use chrono::Timelike;
use matrix_client::{Failure, local};
use matrix_daemon::{config::Store, serve_configured, snapshot};

fn main() {
    let runtime = tokio::runtime::Runtime::new().expect("daemon runtime");
    let result = runtime.block_on(run());
    runtime.shutdown_timeout(std::time::Duration::from_secs(5));
    if let Err(error) = result {
        eprintln!("matrix-daemon: {}", Failure::connecting(&error));
        std::process::exit(1);
    }
}

async fn shutdown_signal() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! { result = tokio::signal::ctrl_c() => result, _ = term.recv() => Ok(()) }
    }
    #[cfg(windows)]
    {
        tokio::signal::ctrl_c().await
    }
}

async fn run() -> std::io::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--probe") {
        println!(
            "{}",
            serde_json::to_string_pretty(&matrix_daemon::probe::report().await)?
        );
        return Ok(());
    }
    let endpoint = local::endpoint()?;
    let listener = local::Listener::bind(&endpoint).await?;
    tokio::spawn(matrix_daemon::suspend::monitor());
    let config = std::sync::Arc::new(std::sync::Mutex::new(Store::load(Store::default_path()?)?));
    let render_config = config.clone();
    let (audio_enable, enabled) = tokio::sync::watch::channel(false);
    let (audio_tx, mut audio_rx) = tokio::sync::watch::channel(None);
    tokio::spawn(matrix_daemon::audio::poll(enabled, audio_tx));
    let feedback_start = std::time::Instant::now();
    let mut feedback = matrix_daemon::feedback::Runtime::default();
    let (display_tx, mut display_rx) = tokio::sync::watch::channel(None);
    let (fast_light, fast_light_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(matrix_daemon::display::poll(display_tx, fast_light_rx));
    let (lock_tx, lock_rx) = tokio::sync::watch::channel(None);
    tokio::spawn(matrix_daemon::session_lock::poll(lock_tx));
    let (power_tx, mut power_rx) = tokio::sync::watch::channel(matrix_daemon::power::unavailable(
        matrix_core::power::PowerStatus::Unavailable,
    ));
    tokio::spawn(matrix_daemon::power::poll(power_tx));
    let (feed_tx, feed_rx) =
        tokio::sync::watch::channel(matrix_core::assistant::AssistantState::default());
    tokio::spawn(matrix_daemon::feeds::poll(feed_tx));
    let now = chrono::Local::now();
    let (tx, rx) = tokio::sync::watch::channel(snapshot(0, now.hour() as u8, now.minute() as u8));
    tokio::spawn(matrix_daemon::devices::route_loop(rx.clone()));
    eprintln!("matrix-daemon: simulated 9x34 panel; local clock; local-only control");
    let clock = async move {
        let mut timer = tokio::time::interval(std::time::Duration::from_secs(1));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut sequence = 0_u32;
        loop {
            tokio::select! {
                _ = timer.tick() => {},
                Ok(()) = audio_rx.changed() => {},
                Ok(()) = display_rx.changed() => {},
                Ok(()) = power_rx.changed() => {},
            }
            let now = chrono::Local::now();
            sequence = sequence.wrapping_add(1);
            let mut frame = snapshot(sequence, now.hour() as u8, now.minute() as u8);
            let power = power_rx.borrow_and_update().clone();
            frame.power = Some(power.clone());
            let assistants = feed_rx.borrow().clone();
            frame.assistants = assistants.clone();
            let (config, preview) = {
                let mut store = render_config.lock().expect("configuration lock");
                let config = store.output();
                (config, store.preview_active(std::time::Instant::now()))
            };
            frame.preview = preview;
            frame.session_locked = *lock_rx.borrow();
            frame.suspend_epoch = matrix_daemon::suspend::epoch();
            frame.display_sources = display_rx.borrow_and_update().clone();
            let config = frame
                .display_sources
                .as_ref()
                .cloned()
                .unwrap_or_default()
                .resolve(config);
            let readings = matrix_core::layout::Readings {
                hour: now.hour() as u8,
                minute: now.minute() as u8,
                default_clock_intensity: config.clock_intensity,
                battery: power.percent.map(f64::from),
                sessions: assistants.sessions,
                ..Default::default()
            };
            frame.frame = matrix_core::layout::render_live(
                &config.panels[0],
                &readings,
                &config.brightness,
                frame.session_locked == Some(true),
            );
            frame.panels = config
                .panels
                .iter()
                .map(|panel| matrix_client::PanelFrame {
                    brightness: matrix_core::brightness::effective(&config.brightness, panel)
                        .level(),
                    name: panel.name.clone(),
                    frame: matrix_core::layout::render_live(
                        panel,
                        &readings,
                        &config.brightness,
                        frame.session_locked == Some(true),
                    ),
                    binding: panel.binding.clone(),
                })
                .collect();
            let needed = config
                .panels
                .iter()
                .any(|p| p.feedback.as_ref().is_some_and(|s| s.volume > 0));
            audio_enable.send_if_modified(|v| {
                let changed = *v != needed;
                *v = needed;
                changed
            });
            let needed = config
                .panels
                .iter()
                .any(|p| p.feedback.as_ref().is_some_and(|s| s.brightness > 0));
            fast_light.send_if_modified(|v| {
                let changed = *v != needed;
                *v = needed;
                changed
            });
            feedback.apply(
                &mut frame,
                &config,
                *audio_rx.borrow_and_update(),
                feedback_start.elapsed(),
            );
            tx.send_replace(frame);
        }
    };
    tokio::select! {
        result = serve_configured(listener, rx, config) => result?,
        _ = clock => {},
        result = shutdown_signal() => result?,
    }
    matrix_daemon::devices::shutdown().await;
    Ok(())
}
