//! Odd epochs pause new panel operations. A resume advances to a new even
//! epoch, so even a full sleep/wake between router ticks invalidates handles.
use std::sync::atomic::{AtomicUsize, Ordering};
static POWER: AtomicUsize = AtomicUsize::new(0);
pub fn epoch() -> usize {
    POWER.load(Ordering::Acquire)
}
pub(crate) fn transition(sleep: bool) {
    let _ = POWER.fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
        ((v % 2 == 1) != sleep).then(|| v.wrapping_add(1))
    });
}

pub async fn monitor() {
    #[cfg(windows)]
    crate::windows_session::notifications().await;
    #[cfg(target_os = "linux")]
    loop {
        linux_monitor().await;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

#[cfg(target_os = "linux")]
async fn linux_monitor() -> Option<()> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};
    let mut child = tokio::process::Command::new("gdbus")
        .args([
            "monitor",
            "--system",
            "--dest",
            "org.freedesktop.login1",
            "--object-path",
            "/org/freedesktop/login1",
        ])
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut lines = BufReader::new(child.stdout.take()?).lines();
    while let Some(line) = lines.next_line().await.ok()? {
        if let Some(args) = line.strip_prefix(
            "/org/freedesktop/login1: org.freedesktop.login1.Manager.PrepareForSleep ",
        ) {
            match args.trim() {
                "(true,)" => transition(true),
                "(false,)" => transition(false),
                _ => {}
            }
        } else if line.starts_with("The name org.freedesktop.login1 is owned by ") {
            // Resync on startup or a monitor/logind restart, including a missed wake.
            if let Some(sleep) = crate::probe::lifecycle::preparing().await {
                transition(sleep);
            }
        }
    }
    let _ = child.wait().await;
    None
}
