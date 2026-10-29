use std::{ffi::OsString, process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn main() {
    let runtime = tokio::runtime::Runtime::new().expect("helper runtime");
    let status = runtime.block_on(run());
    // Optional metadata filesystem work must not hold the status line hostage.
    runtime.shutdown_timeout(Duration::from_millis(100));
    std::process::exit(status);
}

async fn run() -> i32 {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let hook = args.first().is_some_and(|arg| arg == "hook");
    if !hook && !args.first().is_some_and(|arg| arg == "statusline") {
        return 0;
    }
    let downstream = if !hook && args.get(1).is_some_and(|arg| arg == "--") {
        &args[2..]
    } else {
        &[]
    };
    let mut child = if let Some(executable) = downstream.first() {
        match tokio::process::Command::new(executable)
            .args(&downstream[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(child) => Some(child),
            Err(_) => return 0,
        }
    } else {
        None
    };
    let mut sink = child.as_mut().and_then(|child| child.stdin.take());
    let mut capture = Vec::new();
    let mut oversized = false;
    let transfer = async {
        let mut input = tokio::io::stdin();
        let mut buffer = [0; 8192];
        loop {
            let count = input.read(&mut buffer).await?;
            if count == 0 {
                break;
            }
            if !oversized {
                if capture.len() + count <= matrix_claude::MAX_INPUT {
                    capture.extend_from_slice(&buffer[..count]);
                } else {
                    oversized = true;
                    capture.clear();
                }
            }
            if let Some(output) = sink.as_mut()
                && output.write_all(&buffer[..count]).await.is_err()
            {
                sink = None;
            }
        }
        drop(sink.take());
        Ok::<_, std::io::Error>(())
    };
    if !matches!(
        tokio::time::timeout(Duration::from_secs(2), transfer).await,
        Ok(Ok(()))
    ) {
        return 0;
    }
    if !oversized && let Ok(root) = matrix_client::feed_directory() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let _ = tokio::time::timeout(
            Duration::from_millis(150),
            tokio::task::spawn_blocking(move || matrix_claude::update(&root, hook, &capture, now)),
        )
        .await;
    }
    if let Some(mut child) = child {
        match tokio::time::timeout(Duration::from_secs(2), child.wait()).await {
            Ok(Ok(status)) => return status.code().unwrap_or(1),
            _ => {
                let _ = child.kill().await;
            }
        }
    }
    0
}
