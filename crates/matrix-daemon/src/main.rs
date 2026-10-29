use chrono::Timelike;
use matrix_client::{Failure, local};
use matrix_daemon::{config::Store, serve_configured, snapshot};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("matrix-daemon: {}", Failure::connecting(&error));
        std::process::exit(1);
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
    let config = std::sync::Arc::new(std::sync::Mutex::new(Store::load(Store::default_path()?)?));
    let render_config = config.clone();
    let (power_tx, power_rx) = tokio::sync::watch::channel(matrix_daemon::power::unavailable(
        matrix_core::power::PowerStatus::Unavailable,
    ));
    tokio::spawn(matrix_daemon::power::poll(power_tx));
    let now = chrono::Local::now();
    let (tx, rx) = tokio::sync::watch::channel(snapshot(0, now.hour() as u8, now.minute() as u8));
    tokio::spawn(matrix_daemon::devices::route_loop(
        rx.clone(),
        config.clone(),
    ));
    eprintln!("matrix-daemon: simulated 9x34 panel; local clock; local-only control");
    let clock = async move {
        let mut timer = tokio::time::interval(std::time::Duration::from_secs(1));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut sequence = 0_u32;
        loop {
            timer.tick().await;
            let now = chrono::Local::now();
            sequence = sequence.wrapping_add(1);
            let mut frame = snapshot(sequence, now.hour() as u8, now.minute() as u8);
            let power = power_rx.borrow().clone();
            frame.power = Some(power.clone());
            let config = render_config
                .lock()
                .expect("configuration lock")
                .read()
                .config;
            let readings = matrix_core::layout::Readings {
                hour: now.hour() as u8,
                minute: now.minute() as u8,
                default_clock_intensity: config.clock_intensity,
                battery: power.percent.map(f64::from),
                ..Default::default()
            };
            frame.frame = matrix_core::layout::render(&config.panels[0], &readings);
            frame.panels = config
                .panels
                .iter()
                .map(|panel| matrix_client::PanelFrame {
                    name: panel.name.clone(),
                    frame: matrix_core::layout::render(panel, &readings),
                    binding: panel.binding.clone(),
                })
                .collect();
            tx.send_replace(frame);
        }
    };
    tokio::select! {
        result = serve_configured(listener, rx, config) => result?,
        _ = clock => {},
        result = tokio::signal::ctrl_c() => result?,
    }
    Ok(())
}
