//! On-demand samples, not transition notifications. A negative lock hint is
//! not proof of an unlocked compositor, nor is DPMS an optical measurement.
use super::{Capability, State, item};
use std::{fs, io, path::Path, process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;

fn displays(root: &Path) -> io::Result<Vec<Capability>> {
    let mut rows = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|s| {
            s.starts_with("card") && s.contains('-') && matrix_core::display::valid_id(s)
        }) else {
            continue;
        };
        let status = fs::read_to_string(entry.path().join("status"));
        if status
            .as_ref()
            .is_ok_and(|s| matches!(s.trim(), "disconnected" | "unknown"))
        {
            continue;
        }
        let dpms = status
            .ok()
            .filter(|s| s.trim() == "connected")
            .and_then(|_| fs::read_to_string(entry.path().join("dpms")).ok());
        let (state, description) = match dpms.as_deref().map(str::trim) {
            Some("On") => (State::Available, "on"),
            Some("Off") => (State::Available, "off"),
            Some("Standby") => (State::Available, "standby"),
            Some("Suspend") => (State::Available, "DPMS suspend (display only)"),
            _ => (State::NotVerified, "unknown"),
        };
        rows.push(item(
            &format!("display_power:{name}"),
            state,
            &format!("DRM connector power: {description}."),
        ));
    }
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(rows)
}

async fn sample(program: &str, args: &[&str]) -> Option<String> {
    tokio::time::timeout(Duration::from_secs(2), async {
        let mut child = tokio::process::Command::new(program)
            .args(args)
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
            .take(65)
            .read_to_end(&mut bytes)
            .await
            .ok()?;
        if bytes.len() > 64 || !child.wait().await.ok()?.success() {
            return None;
        }
        String::from_utf8(bytes).ok()
    })
    .await
    .ok()
    .flatten()
}

pub(super) async fn report() -> Vec<Capability> {
    let (display, lock, sleep) = tokio::join!(
        tokio::task::spawn_blocking(|| displays(Path::new("/sys/class/drm"))),
        sample(
            "loginctl",
            &["show-session", "auto", "-p", "LockedHint", "--value"]
        ),
        sample(
            "busctl",
            &[
                "--system",
                "get-property",
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                "PreparingForSleep"
            ]
        ),
    );
    let mut rows = match display {
        Ok(Ok(rows)) if !rows.is_empty() => rows,
        Ok(Ok(_)) => vec![item(
            "display_power",
            State::Unavailable,
            "No connected DRM connector reported.",
        )],
        _ => vec![item(
            "display_power",
            State::Error,
            "Could not inspect DRM connector state.",
        )],
    };
    rows.push(match lock.as_deref().map(str::trim) {
        Some("yes") => item(
            "session_lock_hint",
            State::Available,
            "logind reports the session lock hint is set.",
        ),
        Some("no") => item(
            "session_lock_hint",
            State::Available,
            "logind reports the session lock hint is clear.",
        ),
        _ => item(
            "session_lock_hint",
            State::NotVerified,
            "No readable session lock hint.",
        ),
    });
    rows.push(match sleep.as_deref().map(str::trim) {
        Some("b true") => item(
            "preparing_for_sleep",
            State::Available,
            "logind is preparing for system sleep.",
        ),
        Some("b false") => item(
            "preparing_for_sleep",
            State::Available,
            "logind is not preparing for system sleep.",
        ),
        _ => item(
            "preparing_for_sleep",
            State::NotVerified,
            "No readable system sleep preparation state.",
        ),
    });
    rows
}
