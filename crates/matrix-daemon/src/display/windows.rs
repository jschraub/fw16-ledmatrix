//! Match WMI brightness instances by identity, never by enumeration position.
use super::DisplaySources;
use std::{process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;

pub(super) async fn read() -> Option<DisplaySources> {
    let script = include_str!("wmi-brightness.ps1");
    tokio::time::timeout(Duration::from_secs(4), async {
        let mut child = tokio::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .ok()?;
        let mut bytes = Vec::new();
        child
            .stdout
            .take()?
            .take(8193)
            .read_to_end(&mut bytes)
            .await
            .ok()?;
        if bytes.len() > 8192 || !child.wait().await.ok()?.success() {
            return None;
        }
        let rows: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        let mut result = DisplaySources::default();
        for row in rows.as_array()? {
            let id = row.get("id")?.as_str()?;
            if !matrix_core::display::valid_id(id) || result.connected.iter().any(|x| x == id) {
                return None;
            }
            result.connected.push(id.into());
            let level = row
                .get("level")
                .and_then(|l| l.as_u64())
                .filter(|v| *v <= 100)
                .map(|v| v as u8);
            result.backlights.insert(id.into(), level);
        }
        result.connected.sort();
        Some(result)
    })
    .await
    .ok()
    .flatten()
}
