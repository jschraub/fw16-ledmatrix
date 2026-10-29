use matrix_client::capabilities::{Capability, CapabilityReport, CapabilityState as State};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::AsyncReadExt;

fn item(name: &str, state: State, detail: &str) -> Capability {
    Capability {
        name: name.into(),
        state,
        detail: detail.into(),
    }
}

fn presence(name: &str, path: &Path, yes: &str, no: &str) -> Capability {
    match path.try_exists() {
        Ok(true) => item(name, State::Available, yes),
        Ok(false) => item(name, State::Unavailable, no),
        Err(_) => item(
            name,
            State::Error,
            "Cannot inspect location; check permissions. No credential contents were read.",
        ),
    }
}

async fn command(program: &str, args: &[&str]) -> bool {
    let Ok(mut child) = tokio::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
    else {
        return false;
    };
    let outcome = tokio::time::timeout(Duration::from_secs(2), async {
        let mut output = Vec::new();
        child
            .stdout
            .take()
            .expect("piped stdout")
            .take(4096)
            .read_to_end(&mut output)
            .await
            .ok()?;
        let status = child.wait().await.ok()?;
        Some(status.success())
    })
    .await;
    matches!(outcome, Ok(Some(true)))
}

pub async fn report() -> CapabilityReport {
    let mut capabilities = vec![];
    let panel_count = match serialport::available_ports() {
        Ok(ports) => ports
            .iter()
            .filter(|port| {
                matches!(&port.port_type,
            serialport::SerialPortType::UsbPort(info) if info.vid == 0x32ac && info.pid == 0x0020)
            })
            .count() as u32,
        Err(_) => {
            capabilities.push(item(
                "panels",
                State::Error,
                "Serial enumeration failed; check drivers and permissions.",
            ));
            0
        }
    };
    if !capabilities.iter().any(|c| c.name == "panels") {
        capabilities.push(item("panels", if panel_count > 0 { State::Available } else { State::Unavailable },
            if panel_count > 0 { "Framework USB modules detected; port access and physical identity still need testing." } else { "No Framework LED panels detected. Simulated development remains available." }));
    }
    capabilities.push(item("panel_location_identity", State::NotVerified, "Duplicate serial numbers are unsafe identifiers. Verify both bay locations on hardware before assigning roles."));
    let desktop;
    #[cfg(target_os = "linux")]
    {
        let value = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .to_lowercase();
        desktop = ["hyprland", "gnome", "kde", "sway"]
            .into_iter()
            .find(|name| value.contains(name))
            .unwrap_or("unknown")
            .to_owned();
        let backlights = std::fs::read_dir("/sys/class/backlight")
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        capabilities.push(item("screen_brightness", if backlights.is_empty() { State::Unavailable } else { State::Available },
            if backlights.is_empty() { "No sysfs backlight found; use manual brightness." } else { "Sysfs backlight exposed. Display selection and event tracking need adapter qualification." }));
        let power = std::fs::read_dir("/sys/class/power_supply")
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .any(|entry| {
                std::fs::read_to_string(entry.path().join("type"))
                    .is_ok_and(|s| s.trim() == "Battery")
            });
        capabilities.push(item(
            "battery",
            if power {
                State::Available
            } else {
                State::Unavailable
            },
            "Battery presence checked via sysfs; no synthetic zero reading.",
        ));
        let audio = command("pactl", &["info"]).await;
        capabilities.push(item(
            "audio",
            if audio {
                State::Available
            } else {
                State::Unavailable
            },
            "Bounded pactl server probe; server/device names are not retained.",
        ));
        let startup = command("systemctl", &["--user", "show-environment"]).await;
        capabilities.push(item(
            "user_startup",
            if startup {
                State::Available
            } else {
                State::Unavailable
            },
            "User systemd manager reachability, not service installation.",
        ));
        capabilities.push(item("display_lock_suspend", State::NotVerified, "Session-bus/desktop event coverage needs an interactive session test. No listener is claimed active."));
    }
    #[cfg(windows)]
    {
        desktop = "windows-user-session".into();
        let brightness = command("powershell.exe", &["-NoProfile", "-NonInteractive", "-Command", "if (@(Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightness -ErrorAction SilentlyContinue).Count -gt 0) { exit 0 } else { exit 1 }"]).await;
        capabilities.push(item(
            "screen_brightness",
            if brightness {
                State::Available
            } else {
                State::Unavailable
            },
            "Bounded WMI internal-display probe; manual fallback remains available.",
        ));
        let battery = command("powershell.exe", &["-NoProfile", "-NonInteractive", "-Command", "if (@(Get-CimInstance Win32_Battery -ErrorAction SilentlyContinue).Count -gt 0) { exit 0 } else { exit 1 }"]).await;
        capabilities.push(item(
            "battery",
            if battery {
                State::Available
            } else {
                State::Unavailable
            },
            "Bounded Windows battery presence probe.",
        ));
        capabilities.push(item("audio", State::NotVerified, "Core Audio endpoint access needs an interactive user-session probe; CI is not that evidence."));
        capabilities.push(item(
            "user_startup",
            State::NotVerified,
            "Per-user login startup must be installed and verified during packaging.",
        ));
        capabilities.push(item(
            "display_lock_suspend",
            State::NotVerified,
            "Windows user-session notification coverage requires an interactive machine.",
        ));
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        desktop = "unsupported".into();
        capabilities.push(item(
            "platform",
            State::Unavailable,
            "Only Linux and Windows are targets.",
        ));
    }

    if let Some(home) = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
    {
        let claude = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude"));
        capabilities.push(presence("claude_credentials", &claude.join(".credentials.json"), "Credential file exists; compatibility/authentication not verified and contents not read.", "No file credentials detected; other secure storage may require a platform adapter."));
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"));
        capabilities.push(presence(
            "opencode_credentials",
            &data.join("opencode/auth.json"),
            "Credential file exists; contents not read.",
            "No OpenCode credential file at the standard XDG-compatible location.",
        ));
    }
    capabilities.push(item("snapshot_storage", State::NotVerified, "Metadata producers will use user-specific storage and explicit expiry, not assume logout clears files."));
    CapabilityReport {
        os: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
        desktop,
        panel_count,
        capabilities,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presence_distinguishes_absent_present_and_preserves_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credential");
        assert_eq!(
            presence("test", &path, "present", "absent").state,
            State::Unavailable
        );
        std::fs::write(&path, "secret-token-do-not-read").unwrap();
        let found = presence("test", &path, "present", "absent");
        assert_eq!(found.state, State::Available);
        assert!(!serde_json::to_string(&found).unwrap().contains("secret"));
    }
    #[tokio::test]
    async fn reports_platform_without_requiring_panels_or_credentials() {
        let report = report().await;
        assert_eq!(report.os, std::env::consts::OS);
        assert!(
            report
                .capabilities
                .iter()
                .any(|c| c.name == "panel_location_identity" && c.state == State::NotVerified)
        );
    }
}
