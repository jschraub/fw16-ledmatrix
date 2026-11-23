//! Manage only the packaged user unit's future-login enablement.
#[cfg(target_os = "linux")]
mod linux {
    use std::{process::Stdio, time::Duration};

    const UNIT: &str = "fw16-ledmatrix.service";
    const FRAGMENT: &str = "FragmentPath=/usr/lib/systemd/user/fw16-ledmatrix.service";
    const UNAVAILABLE: &str = "Cannot reach the systemd user manager.";

    async fn systemctl(args: &[&str]) -> Result<std::process::Output, &'static str> {
        tokio::time::timeout(
            Duration::from_secs(4),
            tokio::process::Command::new("/usr/bin/systemctl")
                .args(["--user", "--no-pager", "--no-ask-password"])
                .args(args)
                .arg(UNIT)
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| UNAVAILABLE)?
        .map_err(|_| UNAVAILABLE)
    }

    fn state(properties: &str) -> Result<Option<bool>, &'static str> {
        if properties.lines().any(|p| p == "LoadState=not-found") {
            return Ok(None);
        }
        if !properties.lines().any(|p| p == FRAGMENT)
            || !properties.lines().any(|p| p == "LoadState=loaded")
        {
            return Err("Login startup requires the installed, unmasked package service.");
        }
        if properties.lines().any(|p| p == "UnitFileState=enabled") {
            Ok(Some(true))
        } else if properties.lines().any(|p| p == "UnitFileState=disabled") {
            Ok(Some(false))
        } else {
            Err("The service is managed outside this app.")
        }
    }

    async fn inspect() -> Result<Option<bool>, &'static str> {
        let out = systemctl(&["show", "--property=LoadState,FragmentPath,UnitFileState"]).await?;
        let text = std::str::from_utf8(&out.stdout).map_err(|_| UNAVAILABLE)?;
        if !out.status.success() && !text.lines().any(|p| p == "LoadState=not-found") {
            return Err(UNAVAILABLE);
        }
        state(text)
    }

    pub(super) async fn configure(enabled: Option<bool>) -> Result<Option<bool>, &'static str> {
        let current = inspect().await?;
        if let (Some(wanted), Some(before)) = (enabled, current) {
            if before != wanted {
                let action = if wanted { "enable" } else { "disable" };
                if !systemctl(&[action]).await?.status.success() || inspect().await? != Some(wanted)
                {
                    return Err(
                        "Startup preference could not be verified. Check its status and retry.",
                    );
                }
            }
            return Ok(Some(wanted));
        }
        Ok(current)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn only_offer_changes_for_installed_persistent_unit() {
            assert_eq!(state("LoadState=not-found\nUnitFileState="), Ok(None));
            let installed = format!("{FRAGMENT}\nLoadState=loaded\nUnitFileState=");
            assert_eq!(state(&(installed.clone() + "disabled")), Ok(Some(false)));
            assert_eq!(state(&(installed.clone() + "enabled")), Ok(Some(true)));
            assert!(state(&(installed + "enabled-runtime")).is_err());
            assert!(
                state("FragmentPath=/tmp/other.service\nLoadState=loaded\nUnitFileState=enabled")
                    .is_err()
            );
        }
    }
}

#[tauri::command]
pub async fn login_startup(enabled: Option<bool>) -> Result<Option<bool>, &'static str> {
    #[cfg(target_os = "linux")]
    {
        linux::configure(enabled).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = enabled;
        Ok(None)
    }
}
