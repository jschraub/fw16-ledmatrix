use serde::{Deserialize, Serialize};
use std::{fmt, io};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unavailable,
    Incompatible,
    AccessDenied,
    TimedOut,
    InvalidMessage,
    Disconnected,
    AlreadyRunning,
    StartFailed,
    SlowConsumer,
    BridgeUnavailable,
}

impl ErrorCode {
    pub const ALL: [Self; 10] = [
        Self::Unavailable,
        Self::Incompatible,
        Self::AccessDenied,
        Self::TimedOut,
        Self::InvalidMessage,
        Self::Disconnected,
        Self::AlreadyRunning,
        Self::StartFailed,
        Self::SlowConsumer,
        Self::BridgeUnavailable,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Failure {
    pub code: ErrorCode,
    pub message: String,
}

impl Failure {
    /// Diagnostics intentionally contain no OS paths, usernames, payloads or tokens.
    pub fn new(code: ErrorCode) -> Self {
        let message = match code {
            ErrorCode::Unavailable => {
                "No daemon is listening. Start the daemon or retry the connection."
            }
            ErrorCode::Incompatible => {
                "The running daemon uses an incompatible protocol or lacks required capabilities. Update matching binaries; it will not be replaced automatically."
            }
            ErrorCode::AccessDenied => {
                "Access to the local daemon was denied. Check endpoint ownership and user permissions; do not run the app as administrator."
            }
            ErrorCode::TimedOut => {
                "The daemon did not respond in time. Check its health, then retry."
            }
            ErrorCode::InvalidMessage => {
                "The daemon sent an invalid control message. Check that desktop and daemon versions match."
            }
            ErrorCode::Disconnected => {
                "The daemon connection closed. Your local UI state is retained; retry after restarting the daemon."
            }
            ErrorCode::AlreadyRunning => {
                "A daemon or another process already owns this endpoint. Retry connecting; no running process was replaced."
            }
            ErrorCode::StartFailed => {
                "The daemon could not start. Build or install matrix-daemon alongside the desktop executable, then retry."
            }
            ErrorCode::SlowConsumer => {
                "The desktop stopped acknowledging frames. The bounded subscription was closed; retry to receive the latest state."
            }
            ErrorCode::BridgeUnavailable => {
                "The native desktop bridge is unavailable. Open the Tauri application rather than the Vite page in a browser."
            }
        };
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn connecting(error: &io::Error) -> Self {
        Self::new(match error.kind() {
            io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused => ErrorCode::Unavailable,
            io::ErrorKind::PermissionDenied => ErrorCode::AccessDenied,
            io::ErrorKind::TimedOut => ErrorCode::TimedOut,
            io::ErrorKind::InvalidData => ErrorCode::InvalidMessage,
            io::ErrorKind::AddrInUse => ErrorCode::AlreadyRunning,
            _ => ErrorCode::Disconnected,
        })
    }

    pub fn streaming(error: &io::Error) -> Self {
        let mut failure = Self::connecting(error);
        if failure.code == ErrorCode::Unavailable {
            failure = Self::new(ErrorCode::Disconnected);
        }
        failure
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Failure {}
