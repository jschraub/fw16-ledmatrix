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
