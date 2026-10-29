use matrix_client::{Client, local};
use matrix_daemon::{serve, snapshot};
use std::time::Duration;

#[tokio::test]
async fn independent_clients_reopen_to_latest_rust_rendered_frame() {
    let directory = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    let endpoint = directory
        .path()
        .join("panel.sock")
        .to_string_lossy()
        .into_owned();
    #[cfg(windows)]
    let endpoint = format!(
        r"\\.\pipe\matrix-test-{}-{}",
        std::process::id(),
        directory.path().file_name().unwrap().to_string_lossy()
    );
    let listener = local::Listener::bind(&endpoint).unwrap();
    let (tx, rx) = tokio::sync::watch::channel(snapshot(0, 12, 34));
    let task = tokio::spawn(serve(listener, rx));

    tokio::time::timeout(Duration::from_secs(5), async {
        let mut first = Client::connect(&endpoint).await.unwrap();
        assert!(first.hello.simulated);
        assert!(first.hello.capabilities.contains(&"clock_frames".into()));
        assert_eq!(first.next_frame().await.unwrap(), snapshot(0, 12, 34));
        drop(first); // Same disconnect used by closing the desktop; no daemon ownership.
        for sequence in 1..=100 {
            tx.send_replace(snapshot(sequence, 12, 35));
        }
        let mut reopened = Client::connect(&endpoint).await.unwrap();
        assert_eq!(reopened.next_frame().await.unwrap(), snapshot(100, 12, 35));
        let mut second = Client::connect(&endpoint).await.unwrap();
        assert_eq!(second.next_frame().await.unwrap(), snapshot(100, 12, 35));
        tx.send_replace(snapshot(101, 12, 36));
        assert_eq!(reopened.next_frame().await.unwrap(), snapshot(101, 12, 36));
        assert_eq!(second.next_frame().await.unwrap(), snapshot(101, 12, 36));
    })
    .await
    .unwrap();
    task.abort();
    let _ = task.await;
}
