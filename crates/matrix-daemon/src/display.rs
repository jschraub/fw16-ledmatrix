use matrix_core::display::DisplaySources;
use tokio::sync::watch;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
fn scan(drm: &std::path::Path, lights: &std::path::Path) -> DisplaySources {
    use matrix_core::display::valid_id;
    use std::fs;
    let entries = |dir: &std::path::Path| fs::read_dir(dir).into_iter().flatten().flatten();
    let mut result = DisplaySources::default();
    for entry in entries(drm) {
        let name = entry.file_name();
        let Some((card, connector)) = name.to_str().and_then(|n| n.split_once('-')) else {
            continue;
        };
        if !fs::read_to_string(entry.path().join("status")).is_ok_and(|s| s.trim() == "connected") {
            continue;
        }
        // cardN is enumeration order. Key the physical connector by its GPU instead.
        let Ok(device) = fs::canonicalize(drm.join(card).join("device")) else {
            continue;
        };
        let Some(gpu) = device.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let id = format!("{connector}@{gpu}");
        if valid_id(connector) && valid_id(gpu) && id.len() <= 128 {
            if let Ok(power) = fs::read_to_string(entry.path().join("dpms")) {
                if matches!(power.trim(), "Off" | "Suspend" | "Standby") {
                    result.powered_off.push(id.clone());
                }
                if matches!(power.trim(), "On" | "Off" | "Suspend" | "Standby") {
                    result.power_sources.push(id.clone());
                }
            }
            result.connected.push(id);
        }
    }
    result.connected.sort();
    for entry in entries(lights) {
        let name = entry.file_name();
        let Some(id) = name.to_str().filter(|s| valid_id(s)) else {
            continue;
        };
        let level = crate::probe::backlight::reading(&entry.path())
            .ok()
            .map(|(actual, max)| {
                ((u64::from(actual) * 100 + u64::from(max) / 2) / u64::from(max)) as u8
            });
        result.backlights.insert(id.into(), level);
    }
    result
}

pub async fn poll(tx: watch::Sender<Option<DisplaySources>>) {
    #[cfg(not(any(target_os = "linux", windows)))]
    drop(tx);
    #[cfg(windows)]
    loop {
        let mut sources = windows::read().await.unwrap_or_default();
        if let Some(off) = crate::windows_session::console_off() {
            let name = "Windows-console".to_owned();
            sources.power_sources.push(name.clone());
            if off {
                sources.powered_off.push(name);
            }
        }
        tx.send_replace(Some(sources));
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
    #[cfg(target_os = "linux")]
    {
        use std::{path::Path, time::Duration};
        loop {
            let mut task = tokio::task::spawn_blocking(|| {
                scan(
                    Path::new("/sys/class/drm"),
                    Path::new("/sys/class/backlight"),
                )
            });
            match tokio::time::timeout(Duration::from_secs(2), &mut task).await {
                Ok(result) => {
                    tx.send_replace(result.ok());
                }
                Err(_) => {
                    tx.send_replace(None);
                    let _ = task.await; // Only one outstanding scan. Discard stale results.
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}
