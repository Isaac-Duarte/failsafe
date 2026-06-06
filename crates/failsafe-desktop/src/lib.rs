#[cfg(windows)]
mod screen_renderer;

use std::str::FromStr;

use failsafe_core::device::DeviceId;
use failsafe_screen::ScreenViewerClient;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
#[cfg(windows)]
use tauri::{async_runtime, RunEvent, WindowEvent};
use tokio::sync::Mutex;
#[cfg(windows)]
use tracing::warn;
#[cfg(windows)]
use {
    screen_renderer::{ScreenRenderer, ViewportRect},
    std::sync::Arc,
};

struct ScreenShareRuntime {
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    #[cfg(windows)]
    renderer: Arc<ScreenRenderer>,
}

impl ScreenShareRuntime {
    #[cfg(windows)]
    fn new(renderer: Arc<ScreenRenderer>) -> Self {
        Self {
            task: Mutex::new(None),
            renderer,
        }
    }

    #[cfg(not(windows))]
    fn new() -> Self {
        Self {
            task: Mutex::new(None),
        }
    }
}

#[cfg(not(windows))]
#[derive(Clone, serde::Serialize)]
struct ScreenFramePayload {
    jpeg: Vec<u8>,
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
fn screen_viewer_mode() -> &'static str {
    if cfg!(windows) {
        "gpu"
    } else {
        "webview"
    }
}

#[derive(serde::Deserialize)]
#[cfg_attr(not(windows), allow(dead_code))]
struct ViewportBounds {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[tauri::command]
fn set_screen_viewport(
    runtime: State<'_, ScreenShareRuntime>,
    bounds: ViewportBounds,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        runtime.renderer.set_viewport(ViewportRect {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: bounds.height,
        });
    }
    #[cfg(not(windows))]
    let _ = runtime;
    Ok(())
}

#[cfg(windows)]
fn deactivate_renderer(app: &AppHandle, renderer: Arc<ScreenRenderer>) -> Result<(), String> {
    app.run_on_main_thread(move || {
        renderer.deactivate_and_clear();
    })
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn start_screen_share(
    app: AppHandle,
    runtime: State<'_, ScreenShareRuntime>,
    device_id: String,
    device_name: Option<String>,
) -> Result<(), String> {
    stop_screen_share(app.clone(), runtime.clone()).await?;

    let parsed = DeviceId::from_str(&device_id).map_err(|error| error.to_string())?;
    let client = ScreenViewerClient::connect(parsed)
        .await
        .map_err(|error| error.to_string())?;

    if let Some(window) = app.get_webview_window("main") {
        if let Some(name) = device_name.as_deref() {
            let _ = window.set_title(&format!("Failsafe — {name}"));
        }
    }

    #[cfg(windows)]
    {
        runtime.renderer.set_active(true);

        let renderer = runtime.renderer.clone();
        let app_handle = app.clone();
        let task = tokio::spawn(async move {
            let mut frames = client.frames;
            while let Some(jpeg) = frames.recv().await {
                let renderer = renderer.clone();
                let frame_ok = app_handle
                    .run_on_main_thread(move || {
                        if let Err(error) = renderer.submit_jpeg_and_render(&jpeg) {
                            warn!("failed to render screen frame: {error}");
                        }
                    })
                    .is_ok();
                if !frame_ok {
                    break;
                }
            }
            let _ = deactivate_renderer(&app_handle, renderer);
            let _ = app_handle.emit("screen-stopped", ());
        });

        *runtime.task.lock().await = Some(task);
    }

    #[cfg(not(windows))]
    {
        let app_handle = app.clone();
        let task = tokio::spawn(async move {
            let mut frames = client.frames;
            while let Some(jpeg) = frames.recv().await {
                let payload = ScreenFramePayload { jpeg };
                if app_handle.emit("screen-frame", payload).is_err() {
                    break;
                }
            }
            let _ = app_handle.emit("screen-stopped", ());
        });

        *runtime.task.lock().await = Some(task);
    }

    Ok(())
}

#[tauri::command]
async fn stop_screen_share(
    app: AppHandle,
    runtime: State<'_, ScreenShareRuntime>,
) -> Result<(), String> {
    if let Some(task) = runtime.task.lock().await.take() {
        task.abort();
    }

    #[cfg(windows)]
    deactivate_renderer(&app, runtime.renderer.clone())?;
    #[cfg(not(windows))]
    let _ = app;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            start_screen_share,
            stop_screen_share,
            set_screen_viewport,
            screen_viewer_mode,
        ])
        .setup(|app| {
            #[cfg(windows)]
            {
                let window = app
                    .get_webview_window("main")
                    .ok_or_else(|| "main window not found".to_owned())?;
                let renderer = async_runtime::block_on(ScreenRenderer::new(window))
                    .map_err(|error| error.to_string())?;
                app.manage(ScreenShareRuntime::new(Arc::new(renderer)));
            }

            #[cfg(not(windows))]
            {
                app.manage(ScreenShareRuntime::new());
            }

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
            #[cfg(windows)]
            if let RunEvent::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } = event
            {
                if let Some(runtime) = app_handle.try_state::<ScreenShareRuntime>() {
                    runtime.renderer.resize(size.width, size.height);
                }
            }
            #[cfg(not(windows))]
            let _ = (app_handle, event);
        });
}
