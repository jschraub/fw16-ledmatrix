use tokio::sync::watch;

pub async fn poll(tx: watch::Sender<Option<bool>>) {
    #[cfg(not(target_os = "linux"))]
    drop(tx);
    #[cfg(target_os = "linux")]
    {
        if !std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .split(':')
            .any(|desktop| desktop.eq_ignore_ascii_case("hyprland"))
        {
            return;
        }
        loop {
            // Failure isn't an unlock notification. Keep the last known state.
            if let Some(locked) = crate::probe::lifecycle::compositor_locked().await {
                tx.send_replace(Some(locked));
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }
}
