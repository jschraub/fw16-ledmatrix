//! Shared local-control contract and client. No TCP transport is offered.
pub mod capabilities;
pub mod devices;
mod failure;
pub mod flow;
pub mod local;
pub mod startup;
pub use failure::{ErrorCode, Failure};

use matrix_core::Frame;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{io, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use ts_rs::TS;
pub fn feed_directory() -> io::Result<std::path::PathBuf> {
    std::env::var_os("MATRIX_FEEDS")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            dirs::runtime_dir()
                .or_else(dirs::data_local_dir)
                .map(|path| path.join("fw16-matrix/feeds"))
        })
        .ok_or_else(|| io::Error::other("user metadata directory unavailable"))
}

pub const PROTOCOL_VERSION: u16 = 2;
pub const MAX_MESSAGE: usize = 64 * 1024;
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
pub const FRAME_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum Request {
    BeginPreview {
        owner: String,
        config: matrix_core::config::Config,
        seconds: u8,
    },
    CancelPreview {
        owner: String,
    },
    Devices {},
    Identify {
        port: String,
    },
    CancelIdentify {
        port: String,
    },
    Hello {
        protocol_version: u16,
    },
    Subscribe {},
    Probe {},
    RenderDraft {
        config: matrix_core::config::Config,
        sample: bool,
    },
    ReadConfig {},
    ValidateConfig {
        config: matrix_core::config::Config,
    },
    ApplyConfig {
        expected_revision: u32,
        config: matrix_core::config::Config,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Hello {
    pub protocol_version: u16,
    pub daemon_version: String,
    pub simulated: bool,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Snapshot {
    #[serde(default)]
    pub assistants: matrix_core::assistant::AssistantState,
    #[serde(default)]
    pub preview: bool,
    #[serde(default)]
    pub panels: Vec<PanelFrame>,
    #[serde(default)]
    pub power: Option<matrix_core::power::PowerReading>,
    // A bounded u32 is represented exactly as a JavaScript number.
    pub sequence: u32,
    pub hour: u8,
    pub minute: u8,
    pub frame: Frame,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PanelFrame {
    pub brightness: Option<u8>,
    pub name: String,
    pub frame: Frame,
    pub binding: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Preview {
        reply: matrix_core::preview::PreviewReply,
    },
    Devices {
        reply: devices::DeviceReply,
    },
    Rendered {
        frames: Vec<Frame>,
        sample: bool,
    },
    Configuration {
        reply: matrix_core::config::ConfigReply,
    },
    Hello {
        hello: Hello,
    },
    Frame {
        snapshot: Snapshot,
    },
    Error {
        code: ErrorCode,
        message: String,
    },
    Report {
        report: capabilities::CapabilityReport,
    },
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

pub async fn write_message<W: AsyncWrite + Unpin + ?Sized, T: Serialize>(
    writer: &mut W,
    value: &T,
) -> io::Result<()> {
    let payload = serde_json::to_vec(value).map_err(|e| invalid(e.to_string()))?;
    if payload.len() > MAX_MESSAGE {
        return Err(invalid("message too large"));
    }
    writer.write_u32(payload.len() as u32).await?;
    writer.write_all(&payload).await?;
    writer.flush().await
}

pub async fn read_message<R: AsyncRead + Unpin + ?Sized, T: DeserializeOwned>(
    reader: &mut R,
) -> io::Result<T> {
    let size = reader.read_u32().await? as usize;
    if size == 0 || size > MAX_MESSAGE {
        return Err(invalid("invalid message length"));
    }
    let mut bytes = vec![0; size];
    reader.read_exact(&mut bytes).await?;
    serde_json::from_slice(&bytes).map_err(|_| invalid("malformed control message"))
}

pub struct Client {
    stream: local::Stream,
    pub hello: Hello,
}

impl Client {
    pub async fn connect(endpoint: &str) -> Result<Self, Failure> {
        let mut client = Self::handshake(endpoint).await?;
        write_message(&mut client.stream, &Request::Subscribe {})
            .await
            .map_err(|e| Failure::streaming(&e))?;
        Ok(client)
    }

    pub async fn request(endpoint: &str, request: Request) -> Result<Event, Failure> {
        let mut client = Self::handshake(endpoint).await?;
        tokio::time::timeout(Duration::from_secs(20), async {
            write_message(&mut client.stream, &request)
                .await
                .map_err(|e| Failure::streaming(&e))?;
            let event = read_message(&mut client.stream)
                .await
                .map_err(|e| Failure::streaming(&e))?;
            if let Event::Error { code, .. } = event {
                return Err(Failure::new(code));
            }
            Ok(event)
        })
        .await
        .map_err(|_| Failure::new(ErrorCode::TimedOut))?
    }

    async fn handshake(endpoint: &str) -> Result<Self, Failure> {
        tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
            let mut stream = local::connect(endpoint)
                .await
                .map_err(|e| Failure::connecting(&e))?;
            write_message(
                &mut stream,
                &Request::Hello {
                    protocol_version: PROTOCOL_VERSION,
                },
            )
            .await
            .map_err(|e| Failure::streaming(&e))?;
            let hello = match read_message(&mut stream)
                .await
                .map_err(|e| Failure::streaming(&e))?
            {
                Event::Hello { hello }
                    if hello.protocol_version == PROTOCOL_VERSION
                        && hello.simulated
                        && hello.capabilities.iter().any(|c| c == "clock_frames") =>
                {
                    hello
                }
                Event::Error { code, .. } => return Err(Failure::new(code)),
                _ => return Err(Failure::new(ErrorCode::Incompatible)),
            };
            Ok(Self { stream, hello })
        })
        .await
        .map_err(|_| Failure::new(ErrorCode::TimedOut))?
    }

    pub async fn next_frame(&mut self) -> Result<Snapshot, Failure> {
        let event = tokio::time::timeout(FRAME_TIMEOUT, read_message(&mut self.stream))
            .await
            .map_err(|_| Failure::new(ErrorCode::TimedOut))?
            .map_err(|e| Failure::streaming(&e))?;
        match event {
            Event::Frame { snapshot }
                if snapshot.frame.is_valid()
                    && snapshot.hour < 24
                    && snapshot.minute < 60
                    && snapshot.panels.len() <= 2
                    && snapshot.assistants.sessions.len() <= 64
                    && snapshot.assistants.sessions.iter().all(|session| {
                        matrix_core::assistant::safe_id(&session.session_id)
                            && matrix_core::assistant::safe_id(&session.instance_id)
                            && session.context_percent.is_none_or(|p| p <= 1000)
                            && session.last_activity_ms <= 8_640_000_000_000_000
                            && session.heartbeat_ms <= 8_640_000_000_000_000
                    })
                    && snapshot.panels.iter().all(|panel| {
                        panel.frame.is_valid() && !panel.name.is_empty() && panel.name.len() <= 32
                    }) =>
            {
                Ok(snapshot)
            }
            Event::Error { code, .. } => Err(Failure::new(code)),
            _ => Err(Failure::new(ErrorCode::InvalidMessage)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn framed_protocol_round_trips_and_bounds_allocations() {
        let (mut a, mut b) = tokio::io::duplex(128);
        write_message(&mut a, &Request::Subscribe {}).await.unwrap();
        assert!(matches!(
            read_message(&mut b).await.unwrap(),
            Request::Subscribe {}
        ));
        a.write_u32((MAX_MESSAGE + 1) as u32).await.unwrap();
        assert!(read_message::<_, Request>(&mut b).await.is_err());
    }

    #[tokio::test]
    async fn rejects_malformed_empty_truncated_and_unknown_messages_without_echoing_payloads() {
        for payload in [
            b"secret-not-json".as_slice(),
            br#"{"type":"not_real"}"#,
            br#"{"type":"subscribe","token":"secret"}"#,
        ] {
            let (mut writer, mut reader) = tokio::io::duplex(256);
            writer.write_u32(payload.len() as u32).await.unwrap();
            writer.write_all(payload).await.unwrap();
            let error = read_message::<_, Request>(&mut reader).await.unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert!(!error.to_string().contains("secret"));
        }
        let (mut writer, mut reader) = tokio::io::duplex(128);
        writer.write_u32(0).await.unwrap();
        assert!(read_message::<_, Request>(&mut reader).await.is_err());
        writer.write_u32(20).await.unwrap();
        writer.write_all(b"{").await.unwrap();
        drop(writer);
        assert_eq!(
            read_message::<_, Request>(&mut reader)
                .await
                .unwrap_err()
                .kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn diagnostics_do_not_echo_os_paths_or_payloads() {
        let error = io::Error::new(
            io::ErrorKind::PermissionDenied,
            "/private/user/token=secret",
        );
        let failure = Failure::connecting(&error);
        assert_eq!(failure.code, ErrorCode::AccessDenied);
        assert!(!failure.message.contains("secret"));
        assert!(!failure.message.contains("/private"));
    }
}
