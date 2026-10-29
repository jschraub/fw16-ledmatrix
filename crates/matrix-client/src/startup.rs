//! Explicit user-requested startup only. Never kill, replace, or search PATH for a daemon.
use crate::{Client, ErrorCode, Failure};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::process::Command;

pub fn sibling_daemon() -> Result<PathBuf, Failure> {
    let desktop = std::env::current_exe().map_err(|_| Failure::new(ErrorCode::StartFailed))?;
    Ok(desktop
        .parent()
        .ok_or_else(|| Failure::new(ErrorCode::StartFailed))?
        .join(format!("matrix-daemon{}", std::env::consts::EXE_SUFFIX)))
}

/// Returns only after a matching daemon is ready. An occupied, inaccessible,
/// malformed or incompatible endpoint is never treated as permission to spawn.
pub async fn start(executable: &Path, endpoint: &str) -> Result<Option<u32>, Failure> {
    match Client::connect(endpoint).await {
        Ok(_) => return Ok(None),
        Err(failure) if failure.code == ErrorCode::Unavailable => {}
        Err(failure) => return Err(failure),
    }
    let mut command = Command::new(executable);
    command
        .env("MATRIX_ENDPOINT", endpoint)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false);
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW; no inherited console.
    let mut child = command
        .spawn()
        .map_err(|_| Failure::new(ErrorCode::StartFailed))?;
    let started_pid = child.id();
    let result = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            match Client::connect(endpoint).await {
                Ok(_) => return Ok(started_pid),
                Err(failure) if failure.code == ErrorCode::Unavailable => {}
                Err(failure) => return Err(failure),
            }
            if child
                .try_wait()
                .map_err(|_| Failure::new(ErrorCode::StartFailed))?
                .is_some()
            {
                return Err(Failure::new(ErrorCode::StartFailed));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| Err(Failure::new(ErrorCode::TimedOut)));
    // Reap the explicitly spawned child without tying its lifetime to the UI.
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    result
}
