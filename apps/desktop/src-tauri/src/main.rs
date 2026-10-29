#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use matrix_client::{Client, ErrorCode, Event, Failure, flow::Acknowledgements, local, startup};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{State, ipc::Channel};

#[tauri::command]
async fn probe() -> Result<matrix_client::capabilities::CapabilityReport, Failure> {
    let endpoint = local::endpoint().map_err(|e| Failure::connecting(&e))?;
    match Client::request(&endpoint, matrix_client::Request::Probe {}).await? {
        Event::Report { report } => Ok(report),
        _ => Err(Failure::new(ErrorCode::InvalidMessage)),
    }
}

#[derive(Default)]
struct Subscription {
    task: Option<tauri::async_runtime::JoinHandle<()>>,
    acknowledgements: Acknowledgements,
}

#[derive(Default)]
struct Bridge {
    subscription: Arc<Mutex<Subscription>>,
    startup: tokio::sync::Mutex<()>,
}

#[tauri::command]
fn acknowledge(subscription_id: String, sequence: u32, bridge: State<'_, Bridge>) {
    if let Ok(mut subscription) = bridge.subscription.lock() {
        subscription
            .acknowledgements
            .acknowledge(&subscription_id, sequence);
    }
}

#[tauri::command]
fn subscribe(
    on_event: Channel<Event>,
    subscription_id: String,
    bridge: State<'_, Bridge>,
) -> Result<(), Failure> {
    if subscription_id.is_empty()
        || subscription_id.len() > 64
        || !subscription_id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err(Failure::new(ErrorCode::InvalidMessage));
    }
    let state = bridge.subscription.clone();
    let mut subscription = bridge
        .subscription
        .lock()
        .map_err(|_| Failure::new(ErrorCode::BridgeUnavailable))?;
    if let Some(previous) = subscription.task.take() {
        previous.abort();
    }
    subscription
        .acknowledgements
        .replace(subscription_id.clone());
    subscription.task = Some(tauri::async_runtime::spawn(async move {
        let result: Result<(), Failure> = async {
            let endpoint = local::endpoint().map_err(|e| Failure::connecting(&e))?;
            let mut client = Client::connect(&endpoint).await?;
            on_event
                .send(Event::Hello {
                    hello: client.hello.clone(),
                })
                .map_err(|_| Failure::new(ErrorCode::Disconnected))?;
            loop {
                let snapshot = client.next_frame().await?;
                let acknowledgement = state
                    .lock()
                    .map_err(|_| Failure::new(ErrorCode::BridgeUnavailable))?
                    .acknowledgements
                    .arm(&subscription_id, snapshot.sequence)?;
                on_event
                    .send(Event::Frame { snapshot })
                    .map_err(|_| Failure::new(ErrorCode::Disconnected))?;
                // No unbounded WebView event queue: only one frame can be pending.
                tokio::time::timeout(Duration::from_secs(5), acknowledgement)
                    .await
                    .map_err(|_| Failure::new(ErrorCode::SlowConsumer))?
                    .map_err(|_| Failure::new(ErrorCode::Disconnected))?;
            }
        }
        .await;
        if let Err(failure) = result {
            let _ = on_event.send(Event::Error {
                code: failure.code,
                message: failure.message,
            });
        }
    }));
    Ok(())
}

#[tauri::command]
async fn start_daemon(bridge: State<'_, Bridge>) -> Result<(), Failure> {
    let _guard = bridge.startup.lock().await;
    let executable = startup::sibling_daemon()?;
    let endpoint = local::endpoint().map_err(|e| Failure::connecting(&e))?;
    startup::start(&executable, &endpoint).await.map(|_| ())
}

fn main() {
    tauri::Builder::default()
        .manage(Bridge::default())
        .invoke_handler(tauri::generate_handler![
            subscribe,
            acknowledge,
            start_daemon,
            probe
        ])
        .run(tauri::generate_context!())
        .expect("failed to run matrix desktop");
}
