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
        tx.send_replace(read().await);
        tokio::select! {
            _=tokio::time::sleep(std::time::Duration::from_millis(750))=>{},
            r=enabled.changed()=>if r.is_err(){return;}
        }
    }
}
