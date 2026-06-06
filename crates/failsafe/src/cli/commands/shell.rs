use std::io::{self, IsTerminal};
use std::path::PathBuf;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use failsafe::DaemonError;
use failsafe_core::api::DeviceInfo;
use failsafe_core::device::DeviceId;
use inquire::Select;
use tokio::net::UnixStream;

use failsafe::control::{
    ControlRequest, ControlResponse, control_socket_path, recv_response, relay_terminal_io,
    send_request,
};

use crate::cli::context::{config_path_or_default, load_config, server_client_from_config};
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
    let mut stream = UnixStream::connect(control_socket_path()?)
        .await
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound
                || error.kind() == io::ErrorKind::ConnectionRefused
            {
                DaemonError::Config(
                    "daemon is not running; start it with `failsafe run`".to_owned(),
                )
            } else {
                DaemonError::Io(error)
            }
        })?;

    send_request(
        &mut stream,
        &ControlRequest::OpenShell {
            target: target.device_id,
            rows,
            cols,
        },
    )
    .await?;

    match recv_response(&mut stream).await? {
        ControlResponse::Ready => {}
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

fn select_device_interactive(
    self_id: DeviceId,
    devices: &[DeviceInfo],
) -> Result<DeviceInfo, DaemonError> {
    let candidates: Vec<DeviceInfo> = devices
        .iter()
        .filter(|device| device.device_id != self_id)
        .cloned()
        .collect();

    if candidates.is_empty() {
        return Err(DaemonError::Config(
            "no other devices available to connect to".to_owned(),
        ));
    }

    if !io::stdin().is_terminal() {
        return Err(DaemonError::Config(
            "device name required when stdin is not a terminal".to_owned(),
        ));
    }

    let options: Vec<(String, usize)> = candidates
        .iter()
        .enumerate()
        .map(|(index, device)| {
            let status = if device.online { "online" } else { "offline" };
            (format!("{}  [{status}]", device.name), index)
        })
        .collect();
    let labels: Vec<String> = options.iter().map(|(label, _)| label.clone()).collect();

    let selection = Select::new("Select device:", labels)
        .with_help_message("↑/↓ to navigate, enter to select, ctrl+c to cancel")
        .prompt()
        .map_err(|error| DaemonError::Config(error.to_string()))?;

    let index = options
        .iter()
        .find(|(label, _)| label == &selection)
        .map(|(_, index)| *index)
        .expect("selected option must exist");
    Ok(candidates[index].clone())
}

fn terminal_size() -> (u16, u16) {
    crossterm::terminal::size().unwrap_or((24, 80))
}
