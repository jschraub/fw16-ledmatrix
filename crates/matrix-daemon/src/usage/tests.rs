use super::*;
use matrix_core::assistant::Provider::{Claude, OpencodeOpenai};
use rusqlite::Connection;

#[test]
fn reset_missing_duration_and_expiry_are_distinct_from_zero() {
    let payload = br#"{"rate_limit":{"primary_window":{"limit_window_seconds":604800,"used_percent":0,"reset_at":180},"secondary_window":{"limit_window_seconds":86400,"used_percent":70}},"irrelevant":"secret"}"#;
    let result = endpoint::parse(OpencodeOpenai, payload, 100).unwrap();
    assert!(result.five_hour.is_none());
    assert_eq!(result.weekly.unwrap().used_percent, 0);
    assert!(!serde_json::to_string(&result).unwrap().contains("secret"));
    assert_eq!(result.clone().at(180).status, UsageStatus::NoWindows);
    assert_eq!(result.at(400).status, UsageStatus::Expired);
    assert_eq!(
        endpoint::parse(OpencodeOpenai, br#"{"rate_limit":{"renamed":{}}}"#, 100),
        Err(UsageStatus::SchemaError)
    );
}

#[test]
fn transient_failures_never_renew_cache_and_auth_failures_drop_it() {
    let payload = br#"{"five_hour":{"utilization":32.4,"resets_at":"2030-01-01T00:00:00Z"},"seven_day":null}"#;
    let result = endpoint::parse(Claude, payload, 500).unwrap();
    assert_eq!(result.five_hour.unwrap().resets_at, Some(1893456000));
    let stale = after(result.clone(), Err(UsageStatus::SchemaError), 800);
    assert_eq!(stale.five_hour.unwrap().used_percent, 32);
    assert_eq!(stale.clone().at(43700).status, UsageStatus::Expired);
    for status in [UsageStatus::AccountChanged, UsageStatus::Unauthorized] {
        assert!(after(stale.clone(), Err(status), 900).five_hour.is_none());
    }
    assert_eq!(after(stale, Ok(result.clone()), 901), result);
    assert!(login::claude(br#"{"claudeAiOauth":{"accessToken":"test\ninjected"}}"#).is_none());
}

#[test]
fn v2_reads_only_active_oauth_and_invalidates_on_account_switch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("opencode.db");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(r#"CREATE TABLE credential (id TEXT, integration_id TEXT, active INTEGER, value TEXT);
    INSERT INTO credential VALUES ('a','openai',NULL,'{"type":"oauth","access":"test-a","metadata":{"accountID":"one"}}');"#).unwrap();
    let a = login::database(&path).unwrap();
    assert_eq!(a.account.as_deref(), Some("one"));
    db.execute_batch(r#"INSERT INTO credential VALUES ('b','openai',1,'{"type":"oauth","access":"test-b","metadata":{"accountID":"two"}}');"#).unwrap();
    let b = login::database(&path).unwrap();
    assert_eq!(b.account.as_deref(), Some("two"));
    assert_ne!(a.key, b.key);
    db.execute_batch("UPDATE credential SET active=1").unwrap();
    assert!(login::database(&path).is_none());
    db.execute_batch(r#"UPDATE credential SET active=0; UPDATE credential SET active=1,value='{"type":"api","key":"secret"}' WHERE id='b';"#).unwrap();
    assert!(login::database(&path).is_none());
}
