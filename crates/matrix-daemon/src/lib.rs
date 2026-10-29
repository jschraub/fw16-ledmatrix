use matrix_client::{
    ErrorCode, Event, Failure, HANDSHAKE_TIMEOUT, Hello, PROTOCOL_VERSION, Request, Snapshot,
    local, read_message, write_message,
};
use std::sync::Arc;
use std::{io, time::Duration};
use tokio::sync::{Semaphore, watch};
pub mod config;
pub mod devices;
pub mod power;
pub mod probe;
pub type Configuration = Arc<std::sync::Mutex<config::Store>>;

pub const MAX_CLIENTS: usize = 32;

pub fn snapshot(sequence: u32, hour: u8, minute: u8) -> Snapshot {
    Snapshot {
        panels: vec![],
        power: None,
        sequence,
        hour,
        minute,
        frame: matrix_core::render_clock(hour, minute).expect("valid clock time"),
    }
}

/// Watch retains only the latest frame: slow/disconnected clients never queue history.
pub async fn serve(listener: local::Listener, frames: watch::Receiver<Snapshot>) -> io::Result<()> {
    serve_configured(
        listener,
        frames,
        Arc::new(std::sync::Mutex::new(config::Store::memory())),
    )
    .await
}

pub async fn serve_configured(
    mut listener: local::Listener,
    frames: watch::Receiver<Snapshot>,
    config: Configuration,
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
        let config = config.clone();
        tokio::spawn(async move {
            let _permit = permit;
            // A disconnected client does not affect the renderer or other clients.
            let _ = configured_session(stream, frames, config).await;
        });
    }
}

#[cfg(test)]
async fn session(stream: local::Stream, frames: watch::Receiver<Snapshot>) -> io::Result<()> {
    configured_session(
        stream,
        frames,
        Arc::new(std::sync::Mutex::new(config::Store::memory())),
    )
    .await
}

async fn configured_session(
    mut stream: local::Stream,
    mut frames: watch::Receiver<Snapshot>,
    config: Configuration,
) -> io::Result<()> {
    let request = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
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
        read_message::<_, Request>(&mut stream).await
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "handshake timed out"))??;
    if matches!(request, Request::Probe {}) {
        return write_message(
            &mut stream,
            &Event::Report {
                report: probe::report().await,
            },
        )
        .await;
    }
    if matches!(request, Request::Devices {}) {
        return write_message(
            &mut stream,
            &Event::Devices {
                reply: devices::manager().list(),
            },
        )
        .await;
    }
    if let Request::CancelIdentify { ref port } = request {
        devices::manager().cancel(port);
        return write_message(
            &mut stream,
            &Event::Devices {
                reply: devices::manager().list(),
            },
        )
        .await;
    }
    if let Request::Identify { ref port } = request {
        let Some(active) = devices::manager().begin(port) else {
            let mut reply = devices::manager().list();
            reply.message =
                "Device is missing, unsupported or already identifying. Refresh and retry.".into();
            return write_message(&mut stream, &Event::Devices { reply }).await;
        };
        let _cancel = devices::CancelOnDrop(active.clone());
        let port = port.clone();
        let frames = frames.clone();
        let normal_config = config.clone();
        let mut job = tokio::task::spawn_blocking(move || {
            devices::manager().identify(&port, frames, active, normal_config)
        });
        let result = tokio::select! {
            result = &mut job => result,
            _ = read_message::<_, Request>(&mut stream) => return Ok(()),
        };
        let mut reply = devices::manager().list();
        reply.message = if matches!(result, Ok(Ok(()))) { "Identification ended; latest applied frame restored (blank if unassigned)." } else { "Identification failed. Check exclusive access, firmware and connection, then reconnect the panel before retrying." }.into();
        return write_message(&mut stream, &Event::Devices { reply }).await;
    }
    let reply = {
        let mut store = config
            .lock()
            .map_err(|_| io::Error::other("configuration unavailable"))?;
        match request {
            Request::ReadConfig {} => Some(matrix_core::config::ConfigReply {
                status: matrix_core::config::ConfigStatus::Read,
                saved: store.read(),
                errors: vec![],
            }),
            Request::ValidateConfig { ref config } => Some(store.validate(config)),
            Request::ApplyConfig {
                expected_revision,
                ref config,
            } => Some(store.apply(expected_revision, config.clone())),
            _ => None,
        }
    };
    if let Request::RenderDraft { ref config, sample } = request {
        if !config.validate().is_empty() {
            return write_message(
                &mut stream,
                &Event::Error {
                    code: ErrorCode::InvalidMessage,
                    message: Failure::new(ErrorCode::InvalidMessage).message,
                },
            )
            .await;
        }
        use chrono::Timelike;
        let now = chrono::Local::now();
        let mut readings = if sample {
            matrix_core::layout::Readings::sample(now.hour() as u8, now.minute() as u8)
        } else {
            matrix_core::layout::Readings {
                hour: now.hour() as u8,
                minute: now.minute() as u8,
                ..Default::default()
            }
        };
        readings.default_clock_intensity = config.clock_intensity;
        let frames = config
            .panels
            .iter()
            .map(|p| matrix_core::layout::render_configured(p, &readings, &config.brightness))
            .collect();
        return write_message(&mut stream, &Event::Rendered { frames, sample }).await;
    }
    if let Some(reply) = reply {
        return write_message(&mut stream, &Event::Configuration { reply }).await;
    }
    if !matches!(request, Request::Subscribe {}) {
        return Err(io::Error::other("expected command or subscription"));
    }
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
