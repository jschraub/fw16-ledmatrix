use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    #[default]
    Claude,
    OpencodeOpenai,
}
impl Provider {
    pub fn directory(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::OpencodeOpenai => "opencode",
        }
    }
}
pub fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Feed {
    pub schema_version: u16,
    pub instance_id: String,
    pub provider: Provider,
    pub heartbeat_ms: u64,
    pub sessions: Vec<FeedSession>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedSession {
    pub session_id: String,
    pub parent_id: Option<String>,
    pub context_percent: Option<u16>,
    pub busy: bool,
    pub last_activity_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Session {
    pub provider: Provider,
    pub instance_id: String,
    pub session_id: String,
    pub context_percent: Option<u16>,
    pub busy: bool,
    #[ts(type = "number")]
    pub last_activity_ms: u64,
    #[ts(type = "number")]
    pub heartbeat_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FeedHealth {
    pub provider: Provider,
    pub status: String,
    pub detail: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
pub struct AssistantState {
    pub sessions: Vec<Session>,
    pub health: Vec<FeedHealth>,
}

pub fn selected<'a>(
    sessions: &'a [Session],
    provider: Provider,
    pin: Option<&str>,
) -> Option<&'a Session> {
    sessions
        .iter()
        .filter(|session| {
            session.provider == provider && pin.is_none_or(|id| session.session_id == id)
        })
        .max_by(|a, b| {
            (a.last_activity_ms, &a.session_id, &a.instance_id).cmp(&(
                b.last_activity_ms,
                &b.session_id,
                &b.instance_id,
            ))
        })
}
pub fn normalize(feed: Feed, provider: Provider, now: u64) -> Result<Vec<Session>, &'static str> {
    if feed.schema_version != 1
        || feed.provider != provider
        || !safe_id(&feed.instance_id)
        || feed.sessions.len() > 16
        || feed.heartbeat_ms > now.saturating_add(5000)
    {
        return Err("invalid metadata schema, identity, bounds or timestamp");
    }
    if now.saturating_sub(feed.heartbeat_ms) >= 60_000 {
        return Err("stale heartbeat");
    }
    let mut sessions = vec![];
    for session in feed.sessions {
        if !safe_id(&session.session_id)
            || session.parent_id.as_ref().is_some_and(|id| !safe_id(id))
            || session.context_percent.is_some_and(|p| p > 1000)
            || session.last_activity_ms > now.saturating_add(5000)
        {
            return Err("invalid session metadata");
        }
        if session.parent_id.is_some() {
            continue;
        }
        sessions.push(Session {
            provider,
            instance_id: feed.instance_id.clone(),
            session_id: session.session_id,
            context_percent: session.context_percent,
            busy: session.busy,
            last_activity_ms: session.last_activity_ms,
            heartbeat_ms: feed.heartbeat_ms,
        });
    }
    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn heartbeat_does_not_steal_activity_order_and_missing_pin_never_falls_back() {
        let old = Session {
            provider: Provider::Claude,
            instance_id: "a".into(),
            session_id: "older".into(),
            context_percent: Some(30),
            busy: false,
            last_activity_ms: 1,
            heartbeat_ms: 100,
        };
        let recent = Session {
            session_id: "recent".into(),
            last_activity_ms: 2,
            heartbeat_ms: 90,
            ..old.clone()
        };
        let sessions = vec![old, recent];
        assert_eq!(
            selected(&sessions, Provider::Claude, None)
                .unwrap()
                .session_id,
            "recent"
        );
        assert_eq!(
            selected(&sessions, Provider::Claude, Some("older"))
                .unwrap()
                .context_percent,
            Some(30)
        );
        assert!(selected(&sessions, Provider::Claude, Some("expired")).is_none());
        assert!(selected(&sessions, Provider::OpencodeOpenai, None).is_none());
    }
    #[test]
    fn expires_feeds_and_excludes_children_without_reading_conversation_data() {
        let mut feed = Feed {
            schema_version: 1,
            instance_id: "instance".into(),
            provider: Provider::Claude,
            heartbeat_ms: 100_000,
            sessions: vec![FeedSession {
                session_id: "child".into(),
                parent_id: Some("parent".into()),
                context_percent: Some(90),
                busy: true,
                last_activity_ms: 100_000,
            }],
        };
        assert!(
            normalize(feed.clone(), Provider::Claude, 100_000)
                .unwrap()
                .is_empty()
        );
        assert!(normalize(feed.clone(), Provider::Claude, 160_000).is_err());
        feed.instance_id = "../../escape".into();
        assert!(normalize(feed, Provider::Claude, 100_000).is_err());
    }
}
