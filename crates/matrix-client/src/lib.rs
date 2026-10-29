//! Shared local-control contract and client. No TCP transport is offered.
pub mod local;

use matrix_core::Frame;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{io, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use ts_rs::TS;

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_MESSAGE: usize = 64 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Hello { protocol_version: u16 },
    Subscribe,
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
    // A bounded u32 is represented exactly as a JavaScript number.
    pub sequence: u32,
    pub hour: u8,
    pub minute: u8,
    pub frame: Frame,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Hello { hello: Hello },
    Frame { snapshot: Snapshot },
    Error { message: String },
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
    serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))
}

pub struct Client {
    stream: local::Stream,
    pub hello: Hello,
}

impl Client {
    pub async fn connect(endpoint: &str) -> io::Result<Self> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut stream = local::connect(endpoint).await?;
            write_message(
                &mut stream,
                &Request::Hello {
                    protocol_version: PROTOCOL_VERSION,
                },
            )
            .await?;
            let hello = match read_message(&mut stream).await? {
                Event::Hello { hello }
                    if hello.protocol_version == PROTOCOL_VERSION && hello.simulated =>
                {
                    hello
                }
                Event::Error { message } => return Err(invalid(message)),
                _ => return Err(invalid("incompatible daemon handshake")),
            };
            write_message(&mut stream, &Request::Subscribe).await?;
            Ok(Self { stream, hello })
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "daemon handshake timed out"))?
    }

    pub async fn next_frame(&mut self) -> io::Result<Snapshot> {
        match read_message(&mut self.stream).await? {
            Event::Frame { snapshot }
                if snapshot.frame.is_valid() && snapshot.hour < 24 && snapshot.minute < 60 =>
            {
                Ok(snapshot)
            }
            Event::Error { message } => Err(invalid(message)),
            _ => Err(invalid("invalid daemon frame")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn framed_protocol_round_trips_and_bounds_allocations() {
        let (mut a, mut b) = tokio::io::duplex(128);
        write_message(&mut a, &Request::Subscribe).await.unwrap();
        assert!(matches!(
            read_message(&mut b).await.unwrap(),
            Request::Subscribe
        ));
        a.write_u32((MAX_MESSAGE + 1) as u32).await.unwrap();
        assert!(read_message::<_, Request>(&mut b).await.is_err());
    }
}
