use matrix_core::power::{PowerReading, PowerStatus};

pub fn unavailable(status: PowerStatus) -> PowerReading {
    PowerReading {
        status,
        percent: None,
        charging: None,
        ac: None,
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}
#[cfg(target_os = "linux")]
fn read_linux(root: &std::path::Path) -> PowerReading {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return unavailable(PowerStatus::Error),
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    paths.sort();
    let mut ac = None;
    let mut battery = None;
    for path in paths {
        let kind = std::fs::read_to_string(path.join("type")).unwrap_or_default();
        if kind.trim() == "Battery" && battery.is_none() {
            battery = Some(path);
        } else if ["Mains", "USB", "USB_C"].contains(&kind.trim())
            && let Ok(value) = std::fs::read_to_string(path.join("online"))
        {
            match value.trim() {
                "1" => ac = Some(true),
                "0" if ac != Some(true) => ac = Some(false),
                _ => {}
            }
        }
    }
    let Some(path) = battery else {
        let mut reading = unavailable(PowerStatus::Unavailable);
        reading.ac = ac;
        return reading;
    };
    let percent = std::fs::read_to_string(path.join("capacity"))
        .ok()
        .and_then(|value| value.trim().parse::<u8>().ok())
        .filter(|p| *p <= 100);
    if percent.is_none() {
        return unavailable(PowerStatus::Error);
    }
    let charging =
        std::fs::read_to_string(path.join("status"))
            .ok()
            .and_then(|status| match status.trim() {
                "Charging" => Some(true),
                "Discharging" | "Full" | "Not charging" => Some(false),
                _ => None,
            });
    PowerReading {
        status: PowerStatus::Available,
        percent,
        charging,
        ac,
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}

pub fn read() -> PowerReading {
    #[cfg(target_os = "linux")]
    {
        read_linux(std::path::Path::new("/sys/class/power_supply"))
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
        // SAFETY: POD output storage lives through the call; failure is checked.
        let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
        if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
            return unavailable(PowerStatus::Error);
        }
        let percent = (status.BatteryFlag != 128 && status.BatteryLifePercent <= 100)
            .then_some(status.BatteryLifePercent);
        PowerReading {
            status: if percent.is_some() {
                PowerStatus::Available
            } else {
                PowerStatus::Unavailable
            },
            percent,
            charging: if status.BatteryFlag == 255 || status.BatteryFlag == 128 {
                None
            } else {
                Some(status.BatteryFlag & 8 != 0)
            },
            ac: match status.ACLineStatus {
                0 => Some(false),
                1 => Some(true),
                _ => None,
            },
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        unavailable(PowerStatus::Unavailable)
    }
}

pub async fn poll(tx: tokio::sync::watch::Sender<PowerReading>) {
    let mut timer = tokio::time::interval(std::time::Duration::from_secs(5));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        timer.tick().await;
        let mut task = tokio::task::spawn_blocking(read);
        match tokio::time::timeout(std::time::Duration::from_secs(2), &mut task).await {
            Ok(Ok(reading)) => {
                tx.send_replace(reading);
            }
            Ok(Err(_)) => {
                tx.send_replace(unavailable(PowerStatus::Error));
            }
            Err(_) => {
                tx.send_replace(unavailable(PowerStatus::Error));
                // Wait for this one blocking call to recover instead of spawning
                // more wedged threads. The separate clock task keeps running.
                if let Ok(reading) = task.await {
                    tx.send_replace(reading);
                }
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    #[test]
    fn power_fixtures_distinguish_missing_malformed_and_real_readings() {
        let dir = tempfile::tempdir().unwrap();
        let battery = dir.path().join("BAT0");
        let ac = dir.path().join("AC");
        assert_eq!(read_linux(dir.path()).status, PowerStatus::Unavailable);
        std::fs::create_dir(&battery).unwrap();
        std::fs::create_dir(&ac).unwrap();
        for (file, value) in [
            (battery.join("type"), "Battery"),
            (battery.join("capacity"), "74"),
            (battery.join("status"), "Charging"),
            (ac.join("type"), "Mains"),
            (ac.join("online"), "1"),
        ] {
            std::fs::write(file, value).unwrap();
        }
        let reading = read_linux(dir.path());
        assert_eq!(reading.percent, Some(74));
        assert_eq!(reading.charging, Some(true));
        assert_eq!(reading.ac, Some(true));
        std::fs::write(battery.join("capacity"), "255").unwrap();
        let reading = read_linux(dir.path());
        assert_eq!(reading.status, PowerStatus::Error);
        assert_eq!(reading.percent, None);
    }
}
