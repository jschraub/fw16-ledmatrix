//! Read-only Claude input tap. Only explicitly selected scalar metadata hits disk.
use matrix_core::assistant::{Feed, FeedSession, Provider, safe_id};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};

pub const MAX_INPUT: usize = 64 * 1024;

fn private(metadata: &std::fs::Metadata) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: geteuid has no arguments or failure mode.
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private metadata required",
            ));
        }
    }
    let _ = metadata;
    Ok(())
}

fn directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
    }
    #[cfg(windows)]
    std::fs::create_dir_all(path)?;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        return Err(io::Error::other("unsafe directory"));
    }
    private(&metadata)
}

fn open(path: &Path, lock: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    if lock {
        options.write(true).create(true).truncate(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    if std::fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.is_file()) {
        return Err(io::Error::other("unsafe file"));
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("unsafe file"));
    }
    private(&file.metadata()?)?;
    Ok(file)
}

pub fn update(root: &Path, hook: bool, input: &[u8], now: u64) -> io::Result<()> {
    if input.len() > MAX_INPUT {
        return Ok(());
    }
    let payload: serde_json::Value = serde_json::from_slice(input)?;
    let Some(id) = payload["session_id"].as_str().filter(|id| safe_id(id)) else {
        return Ok(());
    };
    // Global hooks can also receive child-agent events. Never let them drive main activity.
    if !payload["agent_id"].is_null() || !payload["parent_id"].is_null() {
        return Ok(());
    }
    let event = if hook {
        payload["hook_event_name"].as_str().unwrap_or("")
    } else {
        "statusline"
    };
    if ![
        "statusline",
        "SessionStart",
        "SessionEnd",
        "UserPromptSubmit",
        "Stop",
        "StopFailure",
    ]
    .contains(&event)
    {
        return Ok(());
    }
    directory(root)?;
    let directory_path = root.join("claude");
    directory(&directory_path)?;
    // Kernel lock works across helper processes. Brief retry also tolerates
    // inherited descriptors between another thread's fork and exec. Never queue
    // or wait indefinitely on a peer.
    let lock = open(&directory_path.join(format!("{id}.lock")), true)?;
    let until = std::time::Instant::now() + std::time::Duration::from_millis(50);
    loop {
        if lock.try_lock().is_ok() {
            break;
        }
        if std::time::Instant::now() >= until {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let path = directory_path.join(format!("{id}.json"));
    let previous = match open(&path, false) {
        Ok(file) => {
            let mut bytes = vec![];
            file.take((MAX_INPUT + 1) as u64).read_to_end(&mut bytes)?;
            if bytes.len() > MAX_INPUT {
                return Ok(());
            }
            let feed: Feed = serde_json::from_slice(&bytes)?;
            if feed.schema_version != 1
                || feed.instance_id != id
                || feed.provider != Provider::Claude
                || feed.sessions.len() > 1
            {
                return Ok(());
            }
            Some(feed)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let mut session = previous
        .as_ref()
        .and_then(|feed| feed.sessions.first())
        .cloned()
        .unwrap_or(FeedSession {
            session_id: id.into(),
            parent_id: None,
            context_percent: None,
            busy: false,
            last_activity_ms: 0,
        });
    if session.session_id != id || session.parent_id.is_some() {
        return Ok(());
    }
    // SessionEnd is a tombstone. A late status-line callback cannot resurrect it.
    if event == "statusline"
        && previous
            .as_ref()
            .is_some_and(|feed| feed.sessions.is_empty())
    {
        return Ok(());
    }
    if now < session.last_activity_ms
        || previous
            .as_ref()
            .is_some_and(|feed| feed.heartbeat_ms > now)
    {
        return Ok(());
    }
    match event {
        "SessionStart" => {
            session.busy = false;
            session.context_percent = None;
            session.last_activity_ms = now;
        }
        "UserPromptSubmit" => {
            session.busy = true;
            session.last_activity_ms = now;
        }
        "Stop" | "StopFailure" => {
            session.busy = false;
            session.last_activity_ms = now;
        }
        "statusline" => {
            session.context_percent = payload["context_window"]["used_percentage"]
                .as_f64()
                .filter(|value| value.is_finite() && (0.0..=1000.0).contains(value))
                .map(|value| value.round() as u16);
        }
        _ => {}
    }
    let feed = Feed {
        schema_version: 1,
        instance_id: id.into(),
        provider: Provider::Claude,
        heartbeat_ms: now,
        sessions: if event == "SessionEnd" {
            vec![]
        } else {
            vec![session]
        },
    };
    let mut file = tempfile::NamedTempFile::new_in(&directory_path)?;
    file.write_all(&serde_json::to_vec(&feed)?)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}
