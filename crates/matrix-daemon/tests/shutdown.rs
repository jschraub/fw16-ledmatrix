#[cfg(unix)]
#[tokio::test]
async fn sigterm_cleans_owned_endpoint_and_exits_successfully() {
    use std::{
        process::{Command, Stdio},
        time::Duration,
    };
    let directory = tempfile::tempdir().unwrap();
    let endpoint = directory
        .path()
        .join("ipc/stop.sock")
        .to_string_lossy()
        .into_owned();
    let mut child = Command::new(env!("CARGO_BIN_EXE_matrix-daemon"))
        .env("MATRIX_ENDPOINT", &endpoint)
        .env("MATRIX_CONFIG", directory.path().join("config.json"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if matrix_client::Client::connect(&endpoint).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        // SAFETY: this PID belongs to the child spawned immediately above.
        assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!std::path::Path::new(&endpoint).exists());
    })
    .await;
    let _ = child.kill();
    let _ = child.wait();
    result.unwrap();
}
