use matrix_client::{
    Event, Hello, PROTOCOL_VERSION, Request, Snapshot, local, read_message, write_message,
};
use std::{io, time::Duration};
use tokio::sync::watch;

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
    loop {
        let stream = listener.accept().await?;
        let frames = frames.clone();
        tokio::spawn(async move {
            // A disconnected client does not affect the renderer or other clients.
            let _ = session(stream, frames).await;
        });
    }
}

async fn session(
    mut stream: local::Stream,
    mut frames: watch::Receiver<Snapshot>,
) -> io::Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
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
                    message: "incompatible protocol version".into(),
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
        if !matches!(read_message(&mut stream).await?, Request::Subscribe) {
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
