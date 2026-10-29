use matrix_core::assistant::{AssistantState, Feed, FeedHealth, Provider, normalize};
use std::{
    io::{self, Read},
    path::Path,
};

fn bytes(path: &Path) -> io::Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::other("not a regular snapshot"));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        // SAFETY: geteuid takes no arguments and cannot fail.
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "snapshot must be user-private",
            ));
        }
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut bytes = vec![];
    options.open(path)?.take(65_537).read_to_end(&mut bytes)?;
    if bytes.len() > 65_536 {
        return Err(io::Error::other("snapshot too large"));
    }
    Ok(bytes)
}

pub fn scan(root: &Path, now: u64) -> AssistantState {
    let mut state = AssistantState::default();
    for provider in [Provider::Claude, Provider::OpencodeOpenai] {
        let directory = root.join(provider.directory());
        let mut sessions = vec![];
        let mut status = "missing";
        let mut errors = 0;
        let mut live = 0;
        match std::fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata.is_dir() => match std::fs::read_dir(&directory) {
                Ok(entries) => {
                    let mut paths = entries
                        .filter_map(Result::ok)
                        .map(|entry| entry.path())
                        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
                        .take(17)
                        .collect::<Vec<_>>();
                    paths.sort();
                    if paths.len() > 16 {
                        errors += 1;
                    }
                    paths.truncate(16);
                    for path in paths {
                        let parsed = bytes(&path)
                            .ok()
                            .and_then(|bytes| serde_json::from_slice::<Feed>(&bytes).ok());
                        let Some(feed) = parsed else {
                            errors += 1;
                            continue;
                        };
                        if path.file_stem().and_then(|name| name.to_str())
                            != Some(&feed.instance_id)
                        {
                            errors += 1;
                            continue;
                        }
                        match normalize(feed, provider, now) {
                            Ok(mut records) => {
                                live += 1;
                                sessions.append(&mut records);
                            }
                            Err("stale heartbeat") => {
                                if status == "missing" {
                                    status = "stale";
                                }
                            }
                            Err(_) => errors += 1,
                        }
                    }
                }
                Err(_) => errors += 1,
            },
            Ok(_) => errors += 1,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => errors += 1,
        }
        if live > 0 {
            status = "live";
        }
        if errors > 0 {
            status = "error_or_limited";
        }
        sessions.sort_by_key(|session| std::cmp::Reverse(session.heartbeat_ms));
        let mut seen = std::collections::BTreeSet::new();
        sessions.retain(|session| seen.insert(session.session_id.clone()));
        sessions.sort_by_key(|session| std::cmp::Reverse(session.last_activity_ms));
        sessions.truncate(32);
        state.sessions.extend(sessions);
        state.health.push(FeedHealth { provider, status: status.into(), detail: "Metadata-only feeds; main sessions only; 60-second heartbeat expiry. Missing pins stay unavailable. Errors/limits affect only this provider.".into() });
    }
    state
}

pub async fn poll(tx: tokio::sync::watch::Sender<AssistantState>) {
    let Ok(root) = matrix_client::feed_directory() else {
        return;
    };
    let mut timer = tokio::time::interval(std::time::Duration::from_secs(1));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        timer.tick().await;
        let root = root.clone();
        let now = chrono::Utc::now().timestamp_millis().max(0) as u64;
        let mut job = tokio::task::spawn_blocking(move || scan(&root, now));
        match tokio::time::timeout(std::time::Duration::from_secs(2), &mut job).await {
            Ok(Ok(state)) => {
                tx.send_replace(state);
            }
            _ => {
                tx.send_replace(AssistantState { sessions: vec![], health: [Provider::Claude, Provider::OpencodeOpenai].into_iter().map(|provider| FeedHealth { provider, status: "error".into(), detail: "Metadata scan failed or timed out; unrelated widgets remain active.".into() }).collect() });
                if !job.is_finished() {
                    let _ = job.await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use matrix_core::assistant::FeedSession;
    #[test]
    fn reads_real_javascript_producer_snapshot_and_expires_after_crash() {
        let root = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../integrations/opencode/test/emit-fixture.mjs");
        let output = std::process::Command::new("node")
            .arg(fixture)
            .arg(root.path())
            .output()
            .expect("Node is a pinned development prerequisite");
        assert!(output.status.success(), "JS producer fixture failed");
        let state = scan(root.path(), 100_000);
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.sessions[0].provider, Provider::OpencodeOpenai);
        assert_eq!(state.sessions[0].context_percent, Some(41));
        assert_eq!(state.sessions[0].last_activity_ms, 90_000);
        assert!(scan(root.path(), 160_000).sessions.is_empty());
    }
    #[test]
    fn producer_reader_contract_is_bounded_private_and_independent_per_provider() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("claude");
        std::fs::create_dir(&directory).unwrap();
        let feed = Feed {
            schema_version: 1,
            instance_id: "fixture".into(),
            provider: Provider::Claude,
            heartbeat_ms: 100_000,
            sessions: vec![FeedSession {
                session_id: "main".into(),
                parent_id: None,
                context_percent: Some(41),
                busy: true,
                last_activity_ms: 90_000,
            }],
        };
        let mut file = tempfile::NamedTempFile::new_in(&directory).unwrap();
        serde_json::to_writer(&mut file, &feed).unwrap();
        file.persist(directory.join("fixture.json")).unwrap();
        let state = scan(root.path(), 100_000);
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.sessions[0].context_percent, Some(41));
        assert!(scan(root.path(), 160_000).sessions.is_empty());
        std::fs::write(directory.join("bad.json"), "secret-conversation").unwrap();
        let state = scan(root.path(), 100_000);
        assert_eq!(state.sessions.len(), 1);
        assert!(!serde_json::to_string(&state).unwrap().contains("secret"));
    }
}
