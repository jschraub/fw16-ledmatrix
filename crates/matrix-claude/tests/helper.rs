use matrix_core::assistant::Provider;
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    root
}
fn invoke(root: &Path, args: &[&str], bytes: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_matrix-claude"))
        .env("MATRIX_FEEDS", root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let bytes = bytes.to_vec();
    let sender = std::thread::spawn(move || {
        let _ = input.write_all(&bytes);
    });
    let output = child.wait_with_output().unwrap();
    sender.join().unwrap();
    output
}
fn event(root: &Path, name: &str, now: u64) {
    matrix_claude::update(
        root,
        true,
        format!(
            r#"{{"session_id":"fixture","hook_event_name":"{name}","prompt":"never-export-me"}}"#
        )
        .as_bytes(),
        now,
    )
    .unwrap();
}
fn status(root: &Path, now: u64) {
    matrix_claude::update(root, false, br#"{"session_id":"fixture","context_window":{"used_percentage":41.4},"transcript_path":"never-export-me"}"#, now).unwrap();
}

#[test]
fn start_working_heartbeat_idle_end_and_late_status_are_explicit() {
    let root = root();
    event(root.path(), "SessionStart", 100_000);
    event(root.path(), "UserPromptSubmit", 101_000);
    status(root.path(), 102_000);
    let state = matrix_daemon::feeds::scan(root.path(), 102_000);
    assert_eq!(state.sessions.len(), 1);
    assert_eq!(state.sessions[0].provider, Provider::Claude);
    assert_eq!(state.sessions[0].context_percent, Some(41));
    assert!(state.sessions[0].busy);
    assert_eq!(state.sessions[0].last_activity_ms, 101_000);
    assert!(
        !serde_json::to_string(&state)
            .unwrap()
            .contains("never-export")
    );
    assert!(
        matrix_daemon::feeds::scan(root.path(), 162_000)
            .sessions
            .is_empty()
    );
    event(root.path(), "StopFailure", 103_000);
    status(root.path(), 104_000);
    assert!(!matrix_daemon::feeds::scan(root.path(), 104_000).sessions[0].busy);
    event(root.path(), "SessionEnd", 105_000);
    status(root.path(), 106_000);
    assert!(
        matrix_daemon::feeds::scan(root.path(), 106_000)
            .sessions
            .is_empty()
    );
    event(root.path(), "SessionStart", 107_000);
    assert_eq!(
        matrix_daemon::feeds::scan(root.path(), 107_000).sessions[0].context_percent,
        None
    );
}

#[test]
fn actual_helper_preserves_raw_stdin_and_downstream_arguments() {
    let root = root();
    for bytes in [
        b"{\"session_id\":\"fixture\"}\r\n\n".to_vec(),
        vec![0, 255, 10, 13, 0],
        vec![b'x'; 131_072],
    ] {
        let output = invoke(
            root.path(),
            &[
                "statusline",
                "--",
                "node",
                "-e",
                "process.stdin.pipe(process.stdout); if(process.argv[1] !== 'path with spaces & quoted') process.exitCode=9",
                "path with spaces & quoted",
            ],
            &bytes,
        );
        assert!(output.status.success());
        assert_eq!(output.stdout, bytes);
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn real_helper_publishes_reader_compatible_metadata_without_stdout_noise() {
    let root = root();
    let output = invoke(
        root.path(),
        &["statusline"],
        br#"{"session_id":"fixture","context_window":{"used_percentage":32},"cwd":"secret"}"#,
    );
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let state = matrix_daemon::feeds::scan(root.path(), now);
    assert_eq!(state.sessions[0].context_percent, Some(32));
    assert_eq!(state.sessions[0].last_activity_ms, 0); // A heartbeat is not activity.
}

#[test]
fn invalid_ids_child_hooks_and_bad_inputs_cannot_write_or_delete_snapshots() {
    let root = root();
    status(root.path(), 100_000);
    for input in [
        br#"{"session_id":"../escape","hook_event_name":"SessionEnd"}"#.as_slice(),
        br#"{"session_id":"fixture","hook_event_name":"SessionEnd","agent_id":"child"}"#,
    ] {
        assert!(invoke(root.path(), &["hook"], input).status.success());
    }
    assert!(
        invoke(root.path(), &["hook"], b"invalid-secret-json")
            .stderr
            .is_empty()
    );
    assert_eq!(
        matrix_daemon::feeds::scan(root.path(), 100_000)
            .sessions
            .len(),
        1
    );
}

#[test]
fn metadata_failure_does_not_break_forwarding_or_surface_errors() {
    let root = root();
    let blocked = root.path().join("blocked");
    std::fs::write(&blocked, "not a directory").unwrap();
    let bytes = br#"{"session_id":"fixture"}"#;
    let output = invoke(
        &blocked,
        &[
            "statusline",
            "--",
            "node",
            "-e",
            "process.stdin.pipe(process.stdout)",
        ],
        bytes,
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, bytes);
    assert!(output.stderr.is_empty());
}

#[test]
fn unknown_context_is_not_zero_and_stale_hook_cannot_reverse_newer_activity() {
    let root = root();
    event(root.path(), "UserPromptSubmit", 100_000);
    event(root.path(), "Stop", 102_000);
    event(root.path(), "UserPromptSubmit", 101_000);
    matrix_claude::update(
        root.path(),
        false,
        br#"{"session_id":"fixture","context_window":{"used_percentage":null}}"#,
        103_000,
    )
    .unwrap();
    let state = matrix_daemon::feeds::scan(root.path(), 103_000);
    assert!(!state.sessions[0].busy);
    assert_eq!(state.sessions[0].context_percent, None);
    assert_eq!(state.sessions[0].last_activity_ms, 102_000);
}

#[test]
fn downstream_failures_preserve_exit_status_and_missing_command_is_quiet() {
    let root = root();
    assert_eq!(
        invoke(
            root.path(),
            &[
                "statusline",
                "--",
                "node",
                "-e",
                "process.exitCode=7;process.stdin.resume()"
            ],
            b"{}"
        )
        .status
        .code(),
        Some(7)
    );
    let missing = invoke(
        root.path(),
        &["statusline", "--", "no-such-matrix-command"],
        b"{}",
    );
    assert!(missing.status.success());
    assert!(missing.stderr.is_empty());
}

#[test]
fn wedged_downstream_has_a_bounded_lifetime() {
    let root = root();
    let started = std::time::Instant::now();
    let output = invoke(
        root.path(),
        &[
            "statusline",
            "--",
            "node",
            "-e",
            "setInterval(()=>{},1000);process.stdin.resume()",
        ],
        b"{}",
    );
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert!(started.elapsed() < std::time::Duration::from_secs(8));
}

#[cfg(unix)]
#[test]
fn symlink_snapshot_is_not_followed_or_repaired() {
    use std::os::unix::fs::symlink;
    let root = root();
    status(root.path(), 100_000);
    let victim = root.path().join("victim");
    std::fs::write(&victim, "secret").unwrap();
    let snapshot = root.path().join("claude/fixture.json");
    std::fs::remove_file(&snapshot).unwrap();
    symlink(&victim, &snapshot).unwrap();
    assert!(
        matrix_claude::update(root.path(), false, br#"{"session_id":"fixture"}"#, 101_000).is_err()
    );
    assert_eq!(std::fs::read_to_string(victim).unwrap(), "secret");
    assert!(snapshot.is_symlink());
}
