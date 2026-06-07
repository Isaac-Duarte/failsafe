use std::io::{self, IsTerminal};
use std::path::PathBuf;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use failsafe::DaemonError;
use failsafe_core::control::connect_control;

use failsafe::control::{
    ControlRequest, ControlResponse, control_socket_path, map_control_connect_error, recv_response,
    relay_terminal_io, send_request,
};
use failsafe_core::feature::FeatureSpec;
use failsafe_shell::{OpenShellRequest, ShellFeatureSpec};

use crate::cli::context::{config_path_or_default, load_config, server_client_from_config};
use crate::cli::device_select::select_device_interactive;
use crate::cli::util::resolve_device_target;

pub async fn shell(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
    device: Option<String>,
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

    let (rows, cols) = terminal_size();
    let mut stream = connect_control(&control_socket_path()?)
        .await
        .map_err(map_control_connect_error)?;

    send_request(
        &mut stream,
        &ControlRequest::new(
            ShellFeatureSpec::feature_id(),
            OpenShellRequest {
                target: target.device_id,
                rows,
                cols,
            },
        )
        .map_err(DaemonError::Control)?,
    )
    .await?;

    match recv_response(&mut stream).await? {
        ControlResponse::Ready => {}
        ControlResponse::CancelTransfers { .. } => {
            return Err(DaemonError::Config(
                "unexpected cancel transfers response".to_owned(),
            ));
        }
        ControlResponse::Error { message } => {
            return Err(DaemonError::Config(message));
        }
    }

    if io::stdin().is_terminal() {
        enable_raw_mode().map_err(DaemonError::Io)?;
    }

    let relay_result = relay_terminal_io(&mut stream).await;

    if io::stdin().is_terminal() {
        let _ = disable_raw_mode();
    }

    relay_result
}

fn terminal_size() -> (u16, u16) {
    crossterm::terminal::size().unwrap_or((24, 80))
}
