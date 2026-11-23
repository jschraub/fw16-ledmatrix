use matrix_core::display::DisplaySources;
use tokio::sync::watch;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
fn automatic_dim() -> Option<bool> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .and_then(|dir| {
            std::fs::metadata(std::path::PathBuf::from(dir).join("matrixd/brightness-auto"))
                .ok()?
                .modified()
                .ok()?
                .elapsed()
                .ok()
        })
        .map(|age| age <= std::time::Duration::from_secs(4))
}

#[cfg(target_os = "linux")]
fn percent(path: &std::path::Path) -> Option<u8> {
    crate::probe::backlight::reading(path)
        .ok()
        .map(|(actual, max)| {
            ((u64::from(actual) * 100 + u64::from(max) / 2) / u64::from(max)) as u8
        })
}

#[cfg(target_os = "linux")]
fn scan(drm: &std::path::Path, lights: &std::path::Path) -> DisplaySources {
    use matrix_core::display::valid_id;
    use std::fs;
    let entries = |dir: &std::path::Path| fs::read_dir(dir).into_iter().flatten().flatten();
    let mut result = DisplaySources {
        automatic_dim: automatic_dim(),
        ..Default::default()
    };
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
        result.backlights.insert(id.into(), percent(&entry.path()));
    }
    result
}

pub async fn poll(tx: watch::Sender<Option<DisplaySources>>, mut feedback: watch::Receiver<bool>) {
    #[cfg(not(target_os = "linux"))]
    let _ = &mut feedback;
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
        let mut discovered = None::<std::time::Instant>;
        loop {
            let cached = tx
                .borrow()
                .clone()
                .filter(|_| discovered.is_some_and(|t| t.elapsed() < Duration::from_secs(2)));
            if cached.is_none() {
                discovered = Some(std::time::Instant::now());
            }
            let mut task = tokio::task::spawn_blocking(move || {
                if let Some(mut sources) = cached {
                    for (id, level) in &mut sources.backlights {
                        *level = percent(&Path::new("/sys/class/backlight").join(id));
                    }
                    sources.automatic_dim = automatic_dim();
                    return sources;
                }
                scan(
                    Path::new("/sys/class/drm"),
                    Path::new("/sys/class/backlight"),
                )
            });
            match tokio::time::timeout(Duration::from_secs(2), &mut task).await {
                Ok(result) => {
                    let next = result.ok();
                    tx.send_if_modified(|prev| {
                        if *prev != next {
                            *prev = next;
                            true
                        } else {
                            false
                        }
                    });
                }
                Err(_) => {
                    tx.send_replace(None);
                    let _ = task.await; // Only one outstanding scan. Discard stale results.
                }
            }
            let delay = if *feedback.borrow_and_update() {
                Duration::from_millis(75)
            } else {
                Duration::from_secs(2)
            };
            tokio::select! {
                _=tokio::time::sleep(delay)=>{},
                r=feedback.changed()=>if r.is_err(){return;}
            }
        }
    }
}
