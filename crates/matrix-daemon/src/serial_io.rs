//! Failure-only serial diagnostics. Never log frame payloads or device paths.
use std::{
    fmt,
    io::{self, Write},
    time::Instant,
};

#[derive(Debug)]
struct CommandFailure {
    stage: &'static str,
    command: u8,
    column: Option<u8>,
    written: usize,
    total: usize,
    elapsed_ms: u128,
    source: io::Error,
}

impl fmt::Display for CommandFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "operation=write stage={} command=0x{:02x} column={:?} written={}/{} elapsed_ms={} kind={:?} os_error={:?}",
            self.stage,
            self.command,
            self.column,
            self.written,
            self.total,
            self.elapsed_ms,
            self.source.kind(),
            self.source.raw_os_error()
        )
    }
}

impl std::error::Error for CommandFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

pub(crate) fn is_command_failure(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|cause| cause.is::<CommandFailure>())
}

/// Equivalent to write_all, retaining progress if a command fails partway.
/// Success means host acceptance, not confirmation of firmware parsing.
pub(crate) fn send(
    writer: &mut (impl Write + ?Sized),
    stage: &'static str,
    command: &[u8],
) -> io::Result<()> {
    let start = Instant::now();
    let mut written = 0;
    while written < command.len() {
        let error = match writer.write(&command[written..]) {
            Ok(0) => io::Error::from(io::ErrorKind::WriteZero),
            Ok(count) => {
                written += count;
                continue;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => error,
        };
        let kind = error.kind();
        let failure = CommandFailure {
            stage,
            command: command[2],
            column: (command[2] == 0x07).then(|| command[3]),
            written,
            total: command.len(),
            elapsed_ms: start.elapsed().as_millis(),
            source: error,
        };
        eprintln!("matrix-daemon: serial failure: {failure}");
        return Err(io::Error::new(kind, failure));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct Writer {
        results: VecDeque<io::Result<usize>>,
        bytes: Vec<u8>,
    }
    impl Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let count = self.results.pop_front().expect("unexpected write")?;
            self.bytes.extend_from_slice(&bytes[..count]);
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            panic!("commands must not flush the output queue")
        }
    }

    #[test]
    fn short_writes_and_interrupts_preserve_command_bytes() {
        let mut writer = Writer {
            results: VecDeque::from([Ok(1), Err(io::ErrorKind::Interrupted.into()), Ok(2), Ok(1)]),
            bytes: vec![],
        };
        let command = [0x32, 0xac, 0x00, 32];
        send(&mut writer, "brightness", &command).unwrap();
        assert_eq!(writer.bytes, command);
        assert!(writer.results.is_empty());
    }

    #[test]
    fn failures_retain_progress_kind_and_os_error_without_payloads() {
        let mut writer = Writer {
            results: VecDeque::from([Ok(2), Err(io::Error::from_raw_os_error(5))]),
            bytes: vec![],
        };
        let error = send(&mut writer, "restore", &[0x32, 0xac, 0x07, 3, 123]).unwrap_err();
        assert_eq!(error.kind(), io::Error::from_raw_os_error(5).kind());
        let message = error.to_string();
        assert!(message.contains("stage=restore command=0x07 column=Some(3) written=2/5"));
        assert!(message.contains("os_error=Some(5)"));
        assert!(!message.contains("123"));
        assert_eq!(writer.bytes, [0x32, 0xac]);
        assert!(writer.results.is_empty());
    }

    #[test]
    fn zero_write_is_a_failure_not_an_infinite_retry() {
        let mut writer = Writer {
            results: VecDeque::from([Ok(0)]),
            bytes: vec![],
        };
        let error = send(&mut writer, "wake", &[0x32, 0xac, 0x03, 0]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WriteZero);
        assert!(error.to_string().contains("written=0/4"));
    }
}
