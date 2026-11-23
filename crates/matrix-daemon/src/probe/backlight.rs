use super::{Capability, State, item};
use std::{fs, io, path::Path};

fn number(path: &Path) -> io::Result<u32> {
    fs::read_to_string(path)?
        .trim()
        .parse()
        .map_err(|_| io::ErrorKind::InvalidData.into())
}

pub(crate) fn reading(path: &Path) -> io::Result<(u32, u32)> {
    let level = number(&path.join("actual_brightness"))?;
    let maximum = number(&path.join("max_brightness"))?;
    if maximum == 0 || level > maximum {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok((level, maximum))
}

pub(super) fn capability(root: &Path) -> Capability {
    let failed = || {
        item(
            "screen_brightness",
            State::Error,
            "Cannot read a valid sysfs backlight level; use manual brightness.",
        )
    };
    let unavailable = || {
        item(
            "screen_brightness",
            State::Unavailable,
            "No sysfs backlight found; use manual brightness.",
        )
    };
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return unavailable(),
        Err(_) => return failed(),
    };
    let mut readings = Vec::new();
    let mut unreadable = false;
    for entry in entries {
        match entry.and_then(|entry| reading(&entry.path())) {
            Ok(value) => readings.push(value),
            Err(_) => unreadable = true,
        }
    }
    let detail = match readings.as_slice() {
        [] if unreadable => return failed(),
        [] => return unavailable(),
        [(level, max)] if !unreadable => format!("Backlight level {level}/{max}."),
        _ => format!("{} readable backlight(s).", readings.len()),
    };
    item(
        "screen_brightness",
        State::Available,
        &format!("{detail} Select a connector and its backlight explicitly to follow brightness."),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distinguishes_absence_failed_reads_and_a_real_zero_level() {
        let root = tempfile::tempdir().unwrap();
        let probe = || capability(root.path());
        assert!(matches!(probe().state, State::Unavailable));
        let path = root.path().join("backlight");
        fs::create_dir(&path).unwrap();
        assert!(matches!(probe().state, State::Error));
        for (actual, max) in [("oops", "100"), ("1", "0"), ("101", "100")] {
            fs::write(path.join("actual_brightness"), actual).unwrap();
            fs::write(path.join("max_brightness"), max).unwrap();
            assert!(matches!(probe().state, State::Error));
        }
        fs::write(path.join("actual_brightness"), "0\n").unwrap();
        let report = probe();
        assert!(matches!(report.state, State::Available));
        assert!(report.detail.contains("0/100"));
        fs::remove_dir_all(path).unwrap();
        assert!(matches!(probe().state, State::Unavailable));
    }
}
