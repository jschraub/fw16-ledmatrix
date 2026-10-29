#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use matrix_client::{Client, Event, local};
use std::sync::Mutex;
use tauri::{State, ipc::Channel};

#[derive(Default)]
struct Subscription(Mutex<Option<tauri::async_runtime::JoinHandle<()>>>);

#[tauri::command]
fn subscribe(
    on_event: Channel<Event>,
    subscription: State<'_, Subscription>,
) -> Result<(), String> {
    let mut task = subscription
        .0
        .lock()
        .map_err(|_| "subscription lock failed")?;
    if let Some(previous) = task.take() {
        previous.abort();
    }
    *task = Some(tauri::async_runtime::spawn(async move {
        let result: Result<(), String> = async {
            let endpoint = local::endpoint().map_err(|e| e.to_string())?;
            let mut client = Client::connect(&endpoint)
                .await
                .map_err(|e| format!("Cannot connect to daemon: {e}"))?;
            on_event
                .send(Event::Hello {
                    hello: client.hello.clone(),
                })
                .map_err(|e| e.to_string())?;
            loop {
                let snapshot = client
                    .next_frame()
                    .await
                    .map_err(|e| format!("Daemon connection closed: {e}"))?;
                on_event
                    .send(Event::Frame { snapshot })
                    .map_err(|e| e.to_string())?;
            }
        }
        .await;
        if let Err(message) = result {
            let _ = on_event.send(Event::Error { message });
        }
    }));
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .manage(Subscription::default())
        .invoke_handler(tauri::generate_handler![subscribe])
        .run(tauri::generate_context!())
        .expect("failed to run matrix desktop");
}
