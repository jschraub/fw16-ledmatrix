//! Linux kernel-backed fault injection; never enumerates or opens physical USB.
use super::*;
use serialport::SerialPort;
use std::os::fd::AsRawFd;

fn stop_output(port: &serialport::TTYPort) {
    // SAFETY: port owns a live terminal descriptor; TCOOFF takes no pointer.
    assert_eq!(unsafe { libc::tcflow(port.as_raw_fd(), libc::TCOOFF) }, 0);
}

#[test]
fn stalled_serial_identification_reports_the_failing_command() {
    let (_peer, mut port) = serialport::TTYPort::pair().unwrap();
    port.set_timeout(Duration::from_millis(250)).unwrap();
    stop_output(&port);
    let start = Instant::now();
    let error = identify_on(
        &mut port,
        &AtomicBool::new(true),
        || panic!("failed identification must not restore"),
        Duration::ZERO,
        32,
    )
    .unwrap_err();
    let elapsed = start.elapsed();
    eprintln!(
        "stalled identification: kind={:?} elapsed_ms={} error={error}",
        error.kind(),
        elapsed.as_millis()
    );
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(elapsed >= Duration::from_millis(200));
    assert!(elapsed < Duration::from_secs(5));
    assert!(error.to_string().contains("command=0x00"));
    assert!(error.to_string().contains("written=0/4"));
    assert!(error.to_string().contains("elapsed_ms="));
}

#[test]
fn stalled_partial_outline_never_sends_restoration() {
    struct StallAfterPrefix {
        port: serialport::TTYPort,
        accepted: Vec<u8>,
    }
    impl Write for StallAfterPrefix {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let remaining = 5_usize.saturating_sub(self.accepted.len());
            if remaining == 0 {
                return self.port.write(bytes);
            }
            let count = self.port.write(&bytes[..bytes.len().min(remaining)])?;
            self.accepted.extend_from_slice(&bytes[..count]);
            if self.accepted.len() == 5 {
                stop_output(&self.port);
            }
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            panic!("failed identification must not flush")
        }
    }

    let (_peer, mut port) = serialport::TTYPort::pair().unwrap();
    port.set_timeout(Duration::from_millis(250)).unwrap();
    let mut writer = StallAfterPrefix {
        port,
        accepted: vec![],
    };
    let error = identify_on(
        &mut writer,
        &AtomicBool::new(true),
        || panic!("failed identification must not restore"),
        Duration::ZERO,
        32,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(
        error
            .to_string()
            .contains("stage=identify-outline command=0x06")
    );
    assert!(error.to_string().contains("written=1/42"));
    assert_eq!(writer.accepted, [0x32, 0xac, 0x00, 32, 0x32]);
}
