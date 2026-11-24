use std::process::Command;

#[test]
fn missing_or_unsupported_login_is_redacted_without_starting_service() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".credentials.json");
    let bytes = br#"{"apiKey":"private-fixture","conversation":"private-text"}"#;
    std::fs::write(&path, bytes).unwrap();
    for provider in ["claude", "openai"] {
        let output = Command::new(env!("CARGO_BIN_EXE_matrix-daemon"))
            .args(["--check-usage", provider])
            .env("CLAUDE_CONFIG_DIR", dir.path())
            .env("OPENCODE_DB", dir.path().join("absent.db"))
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(!text.contains("private"));
        let data: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(data["status"], "unsupported_login");
        assert!(data["weekly"].is_null());
    }
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert!(!dir.path().join("absent.db").exists());
}
