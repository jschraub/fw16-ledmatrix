use chrono::Timelike;
use matrix_client::{Failure, local};
use matrix_daemon::{serve, snapshot};

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
    let now = chrono::Local::now();
    let (tx, rx) = tokio::sync::watch::channel(snapshot(0, now.hour() as u8, now.minute() as u8));
    eprintln!("matrix-daemon: simulated 9x34 panel; local clock; local-only control");
    let clock = async move {
        let mut timer = tokio::time::interval(std::time::Duration::from_secs(1));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut sequence = 0_u32;
        loop {
            timer.tick().await;
            let now = chrono::Local::now();
            sequence = sequence.wrapping_add(1);
            tx.send_replace(snapshot(sequence, now.hour() as u8, now.minute() as u8));
        }
    };
    tokio::select! {
        result = serve(listener, rx) => result?,
        _ = clock => {},
        result = tokio::signal::ctrl_c() => result?,
    }
    Ok(())
}
