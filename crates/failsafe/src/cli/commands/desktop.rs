use std::path::PathBuf;

use failsafe::DaemonError;
use failsafe_core::control::connect_control;
use tokio::io::AsyncReadExt;
use failsafe_core::feature::FeatureSpec;
use failsafe_desktop::{DesktopFeatureSpec, OpenDesktopRequest};

use failsafe::control::{
    ControlRequest, ControlResponse, control_socket_path, map_control_connect_error, recv_response,
    send_request,
};

use crate::cli::context::{config_path_or_default, load_config, server_client_from_config};
use crate::cli::device_select::select_device_interactive;
use crate::cli::util::resolve_device_target;

pub async fn desktop(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
    device: Option<String>,
    view_only: bool,
    display_index: u32,
) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;
    let config = load_config(&path, server_url.clone(), false)?;
    let client = server_client_from_config(Some(path.clone()), server_url).await?;
    let response = client.list_devices().await?;

    let target = match device {
        Some(name) => resolve_device_target(&name, config.device_id, &response.devices)?,
        None => select_device_interactive(config.device_id, &response.devices)?,
    };

    if !target.online {
        return Err(DaemonError::Config(format!(
            "device {} is offline",
            target.name
        )));
    }

    let mut stream = connect_control(&control_socket_path()?)
        .await
        .map_err(map_control_connect_error)?;

    send_request(
        &mut stream,
        &ControlRequest::new(
            DesktopFeatureSpec::feature_id(),
            OpenDesktopRequest {
                target: target.device_id,
                view_only,
                display_index,
            },
        )
        .map_err(DaemonError::Control)?,
    )
    .await?;

    match recv_response(&mut stream).await? {
        ControlResponse::Ready => {
            eprintln!(
                "Desktop session opened to {}. Press Escape in the viewer window to close.",
                target.name
            );
            let mut sink = [0u8; 64];
            while let Ok(read) = stream.read(&mut sink).await {
                if read == 0 {
                    break;
                }
            }
            Ok(())
        }
        ControlResponse::CancelTransfers { .. } => Err(DaemonError::Config(
            "unexpected cancel transfers response".to_owned(),
        )),
        ControlResponse::Error { message } => Err(DaemonError::Config(message)),
    }
}
