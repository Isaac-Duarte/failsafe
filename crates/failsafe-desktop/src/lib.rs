mod screen_renderer;

use std::str::FromStr;
use std::sync::Arc;

use failsafe_core::device::DeviceId;
use failsafe_screen::ScreenViewerClient;
use screen_renderer::ScreenRenderer;
use tauri::{async_runtime, AppHandle, Emitter, Manager, RunEvent, State, WebviewWindow, WindowEvent};
use tokio::sync::Mutex;
use tracing::warn;

struct ScreenShareRuntime {
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    renderer: Arc<ScreenRenderer>,
}

impl ScreenShareRuntime {
    fn new(renderer: Arc<ScreenRenderer>) -> Self {
        Self {
            task: Mutex::new(None),
            renderer,
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

#[tauri::command]
async fn start_screen_share(
    app: AppHandle,
    runtime: State<'_, ScreenShareRuntime>,
    device_id: String,
    device_name: Option<String>,
) -> Result<(), String> {
    stop_screen_share(runtime.clone()).await?;

    let parsed = DeviceId::from_str(&device_id).map_err(|error| error.to_string())?;
    let client = ScreenViewerClient::connect(parsed)
        .await
        .map_err(|error| error.to_string())?;

    if let Some(window) = app.get_webview_window("main") {
        if let Some(name) = device_name.as_deref() {
            let _ = window.set_title(&format!("Failsafe — {name}"));
        }
    }

    let renderer = runtime.renderer.clone();
    let app_handle = app.clone();
    let task = tokio::spawn(async move {
        let mut frames = client.frames;
        while let Some(jpeg) = frames.recv().await {
            let renderer = renderer.clone();
            let render_ok = app_handle
                .run_on_main_thread(move || {
                    if let Err(error) = renderer.submit_jpeg(&jpeg) {
                        warn!("failed to decode screen frame: {error}");
                        return;
                    }
                    if let Err(error) = renderer.render() {
                        warn!("failed to render screen frame: {error}");
                    }
                })
                .is_ok();
            if !render_ok {
                break;
            }
        }
        let _ = app_handle.emit("screen-stopped", ());
    });

    *runtime.task.lock().await = Some(task);
    Ok(())
}

#[tauri::command]
async fn stop_screen_share(runtime: State<'_, ScreenShareRuntime>) -> Result<(), String> {
    if let Some(task) = runtime.task.lock().await.take() {
        task.abort();
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![start_screen_share, stop_screen_share])
        .setup(|app| {
            let window = app
                .get_webview_window("main")
                .ok_or_else(|| "main window not found".to_owned())?;
            let renderer = async_runtime::block_on(ScreenRenderer::new(window))
                .map_err(|error| error.to_string())?;
            app.manage(ScreenShareRuntime::new(Arc::new(renderer)));

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
        .run(|app_handle, event| {
            if let RunEvent::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } = event
            {
                if let Some(runtime) = app_handle.try_state::<ScreenShareRuntime>() {
                    runtime.renderer.resize(size.width, size.height);
                }
            }
        });
}
