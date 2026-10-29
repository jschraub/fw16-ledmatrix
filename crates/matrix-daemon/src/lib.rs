use matrix_client::{
    ErrorCode, Event, Failure, HANDSHAKE_TIMEOUT, Hello, PROTOCOL_VERSION, Request, Snapshot,
    local, read_message, write_message,
};
use std::sync::Arc;
use std::{io, time::Duration};
use tokio::sync::{Semaphore, watch};

pub const MAX_CLIENTS: usize = 32;

pub fn snapshot(sequence: u32, hour: u8, minute: u8) -> Snapshot {
    Snapshot {
        sequence,
        hour,
        minute,
        frame: matrix_core::render_clock(hour, minute).expect("valid clock time"),
    }
}

/// Watch retains only the latest frame: slow/disconnected clients never queue history.
pub async fn serve(
    mut listener: local::Listener,
    frames: watch::Receiver<Snapshot>,
) -> io::Result<()> {
    let permits = Arc::new(Semaphore::new(MAX_CLIENTS));
    loop {
        let stream = match listener.accept().await {
            Ok(stream) => stream,
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => continue,
            Err(e) => return Err(e),
        };
        let Ok(permit) = permits.clone().try_acquire_owned() else {
            drop(stream);
            continue;
        };
        let frames = frames.clone();
        tokio::spawn(async move {
            let _permit = permit;
            // A disconnected client does not affect the renderer or other clients.
            let _ = session(stream, frames).await;
        });
    }
}

async fn session(
    mut stream: local::Stream,
    mut frames: watch::Receiver<Snapshot>,
) -> io::Result<()> {
    tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
        let request = read_message(&mut stream).await?;
        if !matches!(
            request,
            Request::Hello {
                protocol_version: PROTOCOL_VERSION
            }
        ) {
            write_message(
                &mut stream,
                &Event::Error {
                    code: ErrorCode::Incompatible,
                    message: Failure::new(ErrorCode::Incompatible).message,
                },
            )
            .await?;
            return Err(io::Error::other("incompatible protocol"));
        }
        write_message(
            &mut stream,
            &Event::Hello {
                hello: Hello {
                    protocol_version: PROTOCOL_VERSION,
                    daemon_version: env!("CARGO_PKG_VERSION").into(),
                    simulated: true,
                    capabilities: vec!["simulated_panel".into(), "clock_frames".into()],
                },
            },
        )
        .await?;
        if !matches!(read_message(&mut stream).await?, Request::Subscribe {}) {
            return Err(io::Error::other("expected subscription"));
        }
        Ok::<_, io::Error>(())
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "handshake timed out"))??;
    loop {
        let snapshot = frames.borrow_and_update().clone();
        tokio::time::timeout(
            Duration::from_secs(5),
            write_message(&mut stream, &Event::Frame { snapshot }),
        )
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "slow subscriber"))??;
        // Detect client closure even while no new frames are produced.
        tokio::select! {
            changed = frames.changed() => if changed.is_err() { return Ok(()); },
            _ = read_message::<_, Request>(&mut stream) => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn handshake_timeout_is_bounded() {
        let (server, _silent_client) = tokio::io::duplex(64);
        let (_tx, rx) = watch::channel(snapshot(0, 12, 34));
        let error = session(Box::new(server), rx).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }

    #[tokio::test(start_paused = true)]
    async fn slow_subscriber_times_out_without_queueing_frame_history() {
        let (server, mut client) = tokio::io::duplex(256);
        let (tx, rx) = watch::channel(snapshot(0, 12, 34));
        let task = tokio::spawn(session(Box::new(server), rx));
        write_message(
            &mut client,
            &Request::Hello {
                protocol_version: PROTOCOL_VERSION,
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            read_message::<_, Event>(&mut client).await.unwrap(),
            Event::Hello { .. }
        ));
        write_message(&mut client, &Request::Subscribe {})
            .await
            .unwrap();
        // A full 306-pixel frame exceeds this transport's buffer. Never read it.
        for sequence in 1..=100 {
            tx.send_replace(snapshot(sequence, 12, 35));
        }
        assert_eq!(
            task.await.unwrap().unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
    }
}
