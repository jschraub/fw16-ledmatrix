use chrono::Timelike;
use matrix_client::local;
use matrix_daemon::{serve, snapshot};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = local::endpoint()?;
    let listener = local::Listener::bind(&endpoint)?;
    let now = chrono::Local::now();
    let (tx, rx) = tokio::sync::watch::channel(snapshot(0, now.hour() as u8, now.minute() as u8));
    eprintln!("matrix-daemon: simulated 9x34 panel; local clock; endpoint {endpoint}");
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
