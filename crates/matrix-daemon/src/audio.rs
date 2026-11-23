//! Poll actual default-sink values, not notification counts. A failed sample
//! is unknown; the next success establishes a baseline rather than an edge.
use tokio::sync::watch;
pub type Volume = Option<(u8, bool)>;

#[cfg(target_os = "linux")]
async fn query(operation: &str) -> Option<String> {
    use std::{process::Stdio, time::Duration};
    use tokio::io::AsyncReadExt;
    tokio::time::timeout(Duration::from_secs(1), async {
        let mut child = tokio::process::Command::new("pactl")
            .args([operation, "@DEFAULT_SINK@"])
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .ok()?;
        let mut data = Vec::new();
        child
            .stdout
            .take()?
            .take(4097)
            .read_to_end(&mut data)
            .await
            .ok()?;
        if data.len() > 4096 || !child.wait().await.ok()?.success() {
            return None;
        }
        String::from_utf8(data).ok()
    })
    .await
    .ok()?
}
#[cfg(target_os = "linux")]
async fn read() -> Volume {
    let (vol, mute) = tokio::join!(query("get-sink-volume"), query("get-sink-mute"));
    let level = vol?
        .split_whitespace()
        .filter_map(|w| w.strip_suffix('%')?.parse::<u16>().ok())
        .max()?
        .min(100) as u8;
    let muted = match mute?.trim() {
        "Mute: yes" => true,
        "Mute: no" => false,
        _ => return None,
    };
    Some((level, muted))
}
#[cfg(target_os = "linux")]
async fn listen(wake: &tokio::sync::Notify) -> Option<()> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};
    let mut child = tokio::process::Command::new("pactl")
        .arg("subscribe")
        .env("LC_ALL", "C")
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut lines = BufReader::new(child.stdout.take()?).lines();
    while let Some(line) = lines.next_line().await.ok()? {
        // Our own state queries generate client events. Never sample on those.
        if matches!(
            line.split_once(" on ")
                .and_then(|(_, tail)| tail.split_whitespace().next()),
            Some("sink" | "server")
        ) {
            wake.notify_one();
        }
    }
    None
}

#[cfg(target_os = "linux")]
async fn monitor(tx: &watch::Sender<Volume>) {
    use tokio::time::{Duration, sleep};
    let wake = tokio::sync::Notify::new();
    let subscribe = async {
        loop {
            listen(&wake).await;
            sleep(Duration::from_secs(2)).await;
        }
    };
    let sample = async {
        loop {
            let next = read().await;
            tx.send_if_modified(|previous| {
                let changed = *previous != next;
                *previous = next;
                changed
            });
            tokio::select! {
                // Coalesce a burst and allow the default sink state to settle.
                _ = wake.notified() => sleep(Duration::from_millis(20)).await,
                _ = sleep(Duration::from_millis(750)) => {},
            }
        }
    };
    tokio::join!(subscribe, sample);
}

pub async fn poll(mut enabled: watch::Receiver<bool>, tx: watch::Sender<Volume>) {
    #[cfg(not(target_os = "linux"))]
    let _ = (&mut enabled, tx);
    #[cfg(target_os = "linux")]
    loop {
        if !*enabled.borrow_and_update() {
            tx.send_replace(None);
            if enabled.changed().await.is_err() {
                return;
            }
            continue;
        }
        tokio::select! {
            _=monitor(&tx)=>{},
            r=enabled.changed()=>if r.is_err(){return;}
        }
    }
}
