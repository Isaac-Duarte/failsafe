use std::path::PathBuf;

use failsafe::DaemonError;
use failsafe_core::control::connect_control;
use failsafe_lan::{LanControlBody, LanFeatureSpec};

use failsafe::control::{
    ControlRequest, ControlResponse, control_socket_path, map_control_connect_error, recv_response,
    send_request,
};
use crate::cli::context::{config_path_or_default, load_config, server_client_from_config};

pub async fn status(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;
    let config = load_config(&path, server_url.clone(), false)?;
    let client = server_client_from_config(Some(path.clone()), server_url).await?;
    let response = client.list_devices().await?;

    let mut stream = connect_control(&control_socket_path()?)
        .await
        .map_err(map_control_connect_error)?;

    send_request(
        &mut stream,
        &ControlRequest::new(
            LanFeatureSpec::feature_id(),
            LanControlBody::Status,
        )
        .map_err(DaemonError::Control)?,
    )
    .await?;

    match recv_response(&mut stream).await? {
        ControlResponse::LanStatus {
            virtual_ip,
            subnet_cidr,
            interface_up,
            message,
        } => {
            if let Some(message) = message {
                eprintln!("Virtual LAN: {message}");
            } else if let Some(ip) = virtual_ip {
                eprintln!("Virtual IP: {ip}");
                if let Some(subnet) = subnet_cidr {
                    eprintln!("Subnet: {subnet}");
                }
                eprintln!(
                    "Interface: {}",
                    if interface_up { "up" } else { "down" }
                );
            } else {
                eprintln!("Virtual LAN is not active on this device.");
            }
        }
        ControlResponse::Error { message } => {
            return Err(DaemonError::Config(message));
        }
        other => {
            return Err(DaemonError::Config(format!(
                "unexpected control response: {other:?}"
            )));
        }
    }

    eprintln!();
    eprintln!("Family devices:");

    let self_id = config.device_id;
    for device in response.devices {
        if device.device_id == self_id {
            continue;
        }
        let ip = device.virtual_ip.unwrap_or_else(|| "—".to_owned());
        let status = if device.online { "online" } else { "offline" };
        eprintln!("  {}  {}  ({})", device.name, ip, status);
    }

    Ok(())
}
