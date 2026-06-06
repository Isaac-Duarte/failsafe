use std::str::FromStr;

use failsafe_core::device::DeviceId;
use failsafe_screen::{ScreenFramePayload, ScreenQualityPreset, ScreenViewerClient};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tokio::sync::{mpsc, Mutex};

struct ScreenShareRuntime {
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    quality_tx: Mutex<Option<mpsc::Sender<ScreenQualityPreset>>>,
}

impl ScreenShareRuntime {
    fn new() -> Self {
        Self {
            task: Mutex::new(None),
            quality_tx: Mutex::new(None),
        }
    }
}

fn launch_args() -> (Option<String>, Option<String>) {
    let args: Vec<String> = std::env::args().collect();
    let device_id = args
        .iter()
        .position(|arg| arg == "--screen-share")
        .and_then(|index| args.get(index + 1).cloned());
    let device_name = args
        .iter()
        .position(|arg| arg == "--device-name")
        .and_then(|index| args.get(index + 1).cloned());
    (device_id, device_name)
}

fn navigate_to_screen_share(
    window: &WebviewWindow,
    device_id: &str,
    device_name: Option<&str>,
) -> Result<(), String> {
    let script = match device_name {
        Some(name) => {
            let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
            format!(
                "window.location.replace('/screen-share/{device_id}?name=' + encodeURIComponent(\"{escaped}\"));"
            )
        }
        None => format!("window.location.replace('/screen-share/{device_id}');"),
    };
    window.eval(&script).map_err(|error| error.to_string())
}

fn parse_quality_preset(preset: &str) -> Result<ScreenQualityPreset, String> {
    preset.parse().map_err(|error: String| error)
}

#[tauri::command]
async fn start_screen_share(
    app: AppHandle,
    runtime: State<'_, ScreenShareRuntime>,
    device_id: String,
    device_name: Option<String>,
    quality: Option<String>,
) -> Result<(), String> {
    stop_screen_share(app.clone(), runtime.clone()).await?;

    let parsed = DeviceId::from_str(&device_id).map_err(|error| error.to_string())?;
    let client = ScreenViewerClient::connect(parsed)
        .await
        .map_err(|error| error.to_string())?;

    if let Some(quality) = quality.as_deref() {
        let preset = parse_quality_preset(quality)?;
        client
            .set_quality(preset)
            .await
            .map_err(|error| error.to_string())?;
    }

    if let Some(window) = app.get_webview_window("main") {
        if let Some(name) = device_name.as_deref() {
            let _ = window.set_title(&format!("Failsafe — {name}"));
        }
    }

    let quality_tx = client.control_sender();
    let app_handle = app.clone();
    let mut frames = client.frames;
    let task = tokio::spawn(async move {
        while let Some(jpeg) = frames.recv().await {
            let payload = ScreenFramePayload { jpeg };
            if app_handle.emit("screen-frame", payload).is_err() {
                break;
            }
        }
        let _ = app_handle.emit("screen-stopped", ());
    });

    *runtime.quality_tx.lock().await = Some(quality_tx);
    *runtime.task.lock().await = Some(task);

    Ok(())
}

#[tauri::command]
async fn set_screen_quality(
    runtime: State<'_, ScreenShareRuntime>,
    preset: String,
) -> Result<(), String> {
    let parsed = parse_quality_preset(&preset)?;
    let quality_tx = runtime.quality_tx.lock().await;
    let Some(quality_tx) = quality_tx.as_ref() else {
        return Err("screen share session is not active".to_owned());
    };
    quality_tx
        .send(parsed)
        .await
        .map_err(|_| "screen share session is not active".to_owned())
}

#[tauri::command]
async fn stop_screen_share(
    app: AppHandle,
    runtime: State<'_, ScreenShareRuntime>,
) -> Result<(), String> {
    if let Some(task) = runtime.task.lock().await.take() {
        task.abort();
    }
    *runtime.quality_tx.lock().await = None;
    let _ = app;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            start_screen_share,
            stop_screen_share,
            set_screen_quality,
        ])
        .setup(|app| {
            app.manage(ScreenShareRuntime::new());

            let (device_id, device_name) = launch_args();
            if let Some(device_id) = device_id {
                if let Some(window) = app.get_webview_window("main") {
                    navigate_to_screen_share(&window, &device_id, device_name.as_deref())?;
                }
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_app_handle, _event| {});
}
