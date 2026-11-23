//! Bounded firmware-version exchange over an already opened and settled port.
use matrix_core::protocol;
use std::{
    io::{self, Read, Write},
    time::{Duration, Instant},
};

pub(crate) fn query(port: &mut (impl Read + Write + ?Sized)) -> io::Result<String> {
    query_with(port, Duration::from_secs(2), Duration::from_millis(300))
}

fn query_with(
    port: &mut (impl Read + Write + ?Sized),
    response_timeout: Duration,
    retry_delay: Duration,
) -> io::Result<String> {
    // The first query after wake can be lost even after the fade has settled.
    // Retry only a completely unanswered query; never discard partial replies
    // or append another command after a failed (possibly partial) write.
    for attempt in 0..3 {
        crate::serial_io::send(port, "firmware-query", &protocol::command(0x20, &[]))?;
        let started = Instant::now();
        let deadline = started + response_timeout;
        let mut version = [0; 3];
        let mut received = 0;
        while Instant::now() < deadline {
            match port.read(&mut version[received..]) {
                Ok(0) => {
                    return Err(reply_error(
                        io::ErrorKind::UnexpectedEof.into(),
                        attempt,
                        received,
                        started,
                    ));
                }
                Ok(count) => {
                    received += count;
                    if received == version.len() {
                        return protocol::firmware(version).ok_or_else(|| {
                            reply_error(
                                io::Error::other("invalid firmware"),
                                attempt,
                                received,
                                started,
                            )
                        });
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::TimedOut
                            | io::ErrorKind::WouldBlock
                            | io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(reply_error(error, attempt, received, started)),
            }
        }
        if received > 0 {
            return Err(reply_error(
                io::Error::new(io::ErrorKind::TimedOut, "incomplete firmware reply"),
                attempt,
                received,
                started,
            ));
        }
        if attempt < 2 {
            std::thread::sleep(retry_delay);
        } else {
            return Err(reply_error(
                io::Error::new(io::ErrorKind::TimedOut, "firmware did not reply"),
                attempt,
                received,
                started,
            ));
        }
    }
    unreachable!("three attempts return a version or a failure")
}

// Only terminal reply failures are logged, not expected short read timeouts.
fn reply_error(error: io::Error, attempt: usize, received: usize, started: Instant) -> io::Error {
    eprintln!(
        "matrix-daemon: serial failure: operation=read stage=firmware-reply attempt={} received={}/3 elapsed_ms={} kind={:?} os_error={:?}",
        attempt + 1,
        received,
        started.elapsed().as_millis(),
        error.kind(),
        error.raw_os_error()
    );
    error
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct Port {
        queries: usize,
        reply_after: usize,
        reads: VecDeque<io::Result<Vec<u8>>>,
    }

    impl Write for Port {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            assert_eq!(bytes, protocol::command(0x20, &[]));
            self.queries += 1;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Read for Port {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.queries < self.reply_after {
                return Err(io::ErrorKind::TimedOut.into());
            }
            match self
                .reads
                .pop_front()
                .unwrap_or_else(|| Err(io::ErrorKind::TimedOut.into()))
            {
                Ok(bytes) => {
                    assert!(bytes.len() <= buffer.len());
                    buffer[..bytes.len()].copy_from_slice(&bytes);
                    Ok(bytes.len())
                }
                Err(error) => Err(error),
            }
        }
    }

    fn query_fast(port: &mut Port) -> io::Result<String> {
        query_with(port, Duration::from_millis(2), Duration::ZERO)
    }

    #[test]
    fn retries_a_missing_first_reply_after_wake() {
        let mut port = Port {
            queries: 0,
            reply_after: 2,
            reads: VecDeque::from([Ok(vec![0, 0x20, 0])]),
        };
        assert_eq!(query_fast(&mut port).unwrap(), "0.20");
        assert_eq!(port.queries, 2);
    }

    #[test]
    fn preserves_partial_reply_across_read_timeouts() {
        let mut port = Port {
            queries: 0,
            reply_after: 1,
            reads: VecDeque::from([
                Ok(vec![0]),
                Err(io::ErrorKind::TimedOut.into()),
                Ok(vec![0x20, 0]),
            ]),
        };
        assert_eq!(query_fast(&mut port).unwrap(), "0.20");
        assert_eq!(port.queries, 1);
    }

    #[test]
    fn missing_reply_stops_after_three_queries() {
        let mut port = Port {
            queries: 0,
            reply_after: usize::MAX,
            reads: VecDeque::new(),
        };
        assert_eq!(
            query_fast(&mut port).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(port.queries, 3);
    }

    #[test]
    fn incomplete_reply_is_not_discarded_for_another_query() {
        let mut port = Port {
            queries: 0,
            reply_after: 1,
            reads: VecDeque::from([Ok(vec![0])]),
        };
        assert_eq!(
            query_fast(&mut port).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(port.queries, 1);
    }

    #[test]
    fn disconnect_and_invalid_firmware_are_not_retried() {
        for reply in [
            Err(io::ErrorKind::BrokenPipe.into()),
            Ok(vec![]),
            Ok(vec![0, 0xff, 0]),
        ] {
            let mut port = Port {
                queries: 0,
                reply_after: 1,
                reads: VecDeque::from([reply]),
            };
            assert!(query_fast(&mut port).is_err());
            assert_eq!(port.queries, 1);
        }
    }

    #[test]
    fn partial_query_write_is_not_followed_by_another_command() {
        struct Broken {
            bytes: Vec<u8>,
        }
        impl Write for Broken {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if !self.bytes.is_empty() {
                    return Err(io::ErrorKind::BrokenPipe.into());
                }
                self.bytes.push(bytes[0]);
                Ok(1)
            }
            fn flush(&mut self) -> io::Result<()> {
                unreachable!()
            }
        }
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                unreachable!()
            }
        }
        let mut port = Broken { bytes: vec![] };
        assert_eq!(
            query(&mut port).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
        assert_eq!(port.bytes, [0x32]);
    }
}
