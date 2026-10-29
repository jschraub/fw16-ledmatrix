//! A real daemon process survives its clients, independent of a test server task.
use matrix_client::{Client, local};
use std::{
    process::{Child, Command, Stdio},
    time::Duration,
};

struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn daemon_process_survives_close_reopen_and_keeps_ticking() {
    let directory = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    #[cfg(unix)]
    let endpoint = directory
        .path()
        .join("daemon.sock")
        .to_string_lossy()
        .into_owned();
    #[cfg(windows)]
    let endpoint = format!(
        r"\\.\pipe\matrix-process-{}-{}",
        std::process::id(),
        directory.path().file_name().unwrap().to_string_lossy()
    );
    let mut daemon = Daemon(
        Command::new(env!("CARGO_BIN_EXE_matrix-daemon"))
            .env("MATRIX_ENDPOINT", &endpoint)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );

    tokio::time::timeout(Duration::from_secs(15), async {
        let mut first = loop {
            match Client::connect(&endpoint).await {
                Ok(client) => break client,
                Err(_) => {
                    assert!(
                        daemon.0.try_wait().unwrap().is_none(),
                        "daemon exited before connection"
                    );
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        };
        let before = first.next_frame().await.unwrap();
        assert!(first.hello.simulated);
        let duplicate = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::process::Command::new(env!("CARGO_BIN_EXE_matrix-daemon"))
                .env("MATRIX_ENDPOINT", &endpoint)
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(!duplicate.status.success());
        let diagnostics = String::from_utf8(duplicate.stderr).unwrap();
        assert!(!diagnostics.contains(&endpoint));
        // An already-running compatible daemon must not spawn even a missing binary.
        assert!(
            matrix_client::startup::start(std::path::Path::new("never-execute-this"), &endpoint)
                .await
                .unwrap()
                .is_none()
        );
        drop(first);
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(daemon.0.try_wait().unwrap().is_none());
        let mut reopened = Client::connect(&endpoint).await.unwrap();
        let after = reopened.next_frame().await.unwrap();
        assert!(after.sequence > before.sequence);
        assert_eq!(
            after.frame,
            matrix_core::render_clock(after.hour, after.minute).unwrap()
        );
        assert!(local::connect(&endpoint).await.is_ok());
    })
    .await
    .unwrap();
}

struct StartedDaemon(u32);

impl Drop for StartedDaemon {
    fn drop(&mut self) {
        // Only terminate the exact child PID returned by this test's explicit start.
        #[cfg(unix)]
        unsafe {
            libc::kill(self.0 as i32, libc::SIGKILL);
        }
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::{
                Foundation::CloseHandle,
                System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess},
            };
            let process = OpenProcess(PROCESS_TERMINATE, 0, self.0);
            if !process.is_null() {
                TerminateProcess(process, 0);
                CloseHandle(process);
            }
        }
    }
}

#[tokio::test]
async fn explicit_start_and_hard_crash_recovery_use_the_same_safe_endpoint() {
    let directory = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    let endpoint = directory
        .path()
        .join("ipc")
        .join("started.sock")
        .to_string_lossy()
        .into_owned();
    #[cfg(windows)]
    let endpoint = format!(
        r"\\.\pipe\matrix-startup-{}-{}",
        std::process::id(),
        directory.path().file_name().unwrap().to_string_lossy()
    );
    let binary = std::path::Path::new(env!("CARGO_BIN_EXE_matrix-daemon"));
    tokio::time::timeout(Duration::from_secs(20), async {
        let pid = matrix_client::startup::start(binary, &endpoint)
            .await
            .unwrap()
            .unwrap();
        let started = StartedDaemon(pid);
        let mut client = Client::connect(&endpoint).await.unwrap();
        assert!(client.next_frame().await.unwrap().frame.is_valid());
        drop(client);
        drop(started); // A crash leaves a stale Unix socket but releases its OS lock.
        loop {
            if Client::connect(&endpoint).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let restarted = StartedDaemon(
            matrix_client::startup::start(binary, &endpoint)
                .await
                .unwrap()
                .unwrap(),
        );
        let mut client = Client::connect(&endpoint).await.unwrap();
        assert!(client.next_frame().await.unwrap().frame.is_valid());
        drop(client);
        drop(restarted);
    })
    .await
    .unwrap();
}
