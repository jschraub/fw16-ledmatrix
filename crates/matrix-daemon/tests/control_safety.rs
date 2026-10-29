use matrix_client::{
    Client, ErrorCode, Event, Hello, PROTOCOL_VERSION, Request, local, read_message, startup,
    write_message,
};
use matrix_daemon::{MAX_CLIENTS, serve, snapshot};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

fn fixture() -> (tempfile::TempDir, String) {
    let directory = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    #[cfg(unix)]
    let endpoint = directory
        .path()
        .join("control.sock")
        .to_string_lossy()
        .into_owned();
    #[cfg(windows)]
    let endpoint = format!(
        r"\\.\pipe\matrix-control-{}-{}",
        std::process::id(),
        directory.path().file_name().unwrap().to_string_lossy()
    );
    (directory, endpoint)
}

#[tokio::test]
async fn rejects_incompatible_client_and_keeps_serving_other_clients() {
    let (_dir, endpoint) = fixture();
    let listener = local::Listener::bind(&endpoint).await.unwrap();
    let (_tx, rx) = tokio::sync::watch::channel(snapshot(0, 12, 34));
    let task = tokio::spawn(serve(listener, rx));
    let mut stream = local::connect(&endpoint).await.unwrap();
    write_message(
        &mut stream,
        &Request::Hello {
            protocol_version: PROTOCOL_VERSION + 1,
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        read_message::<_, Event>(&mut stream).await.unwrap(),
        Event::Error {
            code: ErrorCode::Incompatible,
            ..
        }
    ));
    let mut valid = Client::connect(&endpoint).await.unwrap();
    assert_eq!(valid.next_frame().await.unwrap(), snapshot(0, 12, 34));
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn incompatible_daemon_is_reported_and_explicit_start_does_not_replace_it() {
    let (_dir, endpoint) = fixture();
    let mut listener = local::Listener::bind(&endpoint).await.unwrap();
    let task = tokio::spawn(async move {
        for _ in 0..2 {
            let mut stream = listener.accept().await.unwrap();
            let _: Request = read_message(&mut stream).await.unwrap();
            write_message(
                &mut stream,
                &Event::Hello {
                    hello: Hello {
                        protocol_version: PROTOCOL_VERSION + 1,
                        daemon_version: "future".into(),
                        simulated: true,
                        capabilities: vec!["clock_frames".into()],
                    },
                },
            )
            .await
            .unwrap();
        }
        listener // retain endpoint until the test explicitly drops it
    });
    assert_eq!(
        Client::connect(&endpoint).await.err().unwrap().code,
        ErrorCode::Incompatible
    );
    // A nonexistent executable would produce StartFailed if startup tried to spawn.
    let missing = std::path::Path::new("/not-a-daemon/never-run");
    assert_eq!(
        startup::start(missing, &endpoint).await.unwrap_err().code,
        ErrorCode::Incompatible
    );
    let listener = task.await.unwrap();
    assert!(local::Listener::bind(&endpoint).await.is_err());
    drop(listener);
}

#[tokio::test]
async fn malformed_remote_error_diagnostics_are_not_trusted() {
    let (_dir, endpoint) = fixture();
    let mut listener = local::Listener::bind(&endpoint).await.unwrap();
    let task = tokio::spawn(async move {
        let mut stream = listener.accept().await.unwrap();
        let _: Request = read_message(&mut stream).await.unwrap();
        write_message(
            &mut stream,
            &Event::Error {
                code: ErrorCode::Incompatible,
                message: "/private/token=secret".into(),
            },
        )
        .await
        .unwrap();
    });
    let failure = Client::connect(&endpoint).await.err().unwrap();
    assert_eq!(failure.code, ErrorCode::Incompatible);
    assert!(!failure.message.contains("secret"));
    task.await.unwrap();
}

#[tokio::test]
async fn admission_limit_is_bounded_and_released_by_disconnects() {
    let (_dir, endpoint) = fixture();
    let listener = local::Listener::bind(&endpoint).await.unwrap();
    let (_tx, rx) = tokio::sync::watch::channel(snapshot(0, 12, 34));
    let task = tokio::spawn(serve(listener, rx));
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut clients = Vec::new();
        for _ in 0..MAX_CLIENTS {
            clients.push(Client::connect(&endpoint).await.unwrap());
        }
        assert!(Client::connect(&endpoint).await.is_err());
        drop(clients);
        loop {
            if let Ok(mut client) = Client::connect(&endpoint).await {
                assert_eq!(client.next_frame().await.unwrap(), snapshot(0, 12, 34));
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn malformed_and_oversized_sessions_cannot_crash_the_daemon() {
    let (_dir, endpoint) = fixture();
    let listener = local::Listener::bind(&endpoint).await.unwrap();
    let (_tx, rx) = tokio::sync::watch::channel(snapshot(0, 12, 34));
    let task = tokio::spawn(serve(listener, rx));
    tokio::time::timeout(Duration::from_secs(5), async {
        for (length, payload) in [
            (0, b"".as_slice()),
            (65_537, b"".as_slice()),
            (7, b"invalid".as_slice()),
        ] {
            let mut stream = local::connect(&endpoint).await.unwrap();
            stream.write_u32(length).await.unwrap();
            stream.write_all(payload).await.unwrap();
            stream.shutdown().await.unwrap();
            assert!(read_message::<_, Event>(&mut stream).await.is_err());
        }
        // Named pipes do not support Unix-style write-half shutdown. A truly
        // truncated request is a disconnected client, not a half-closed pipe.
        let mut truncated = local::connect(&endpoint).await.unwrap();
        truncated.write_u32(10).await.unwrap();
        truncated.write_all(b"{").await.unwrap();
        drop(truncated);
        let mut valid = Client::connect(&endpoint).await.unwrap();
        assert_eq!(valid.next_frame().await.unwrap(), snapshot(0, 12, 34));
    })
    .await
    .unwrap();
    task.abort();
    let _ = task.await;
}
