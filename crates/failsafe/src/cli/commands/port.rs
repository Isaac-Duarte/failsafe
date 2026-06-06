use std::io::{self, IsTerminal};
use std::path::PathBuf;

use failsafe::DaemonError;
use failsafe_core::control::PortProtocol;
use failsafe_core::control::connect_control;
use inquire::Text;
use tokio::io::AsyncReadExt;

use failsafe::control::{
    ControlRequest, ControlResponse, control_socket_path, map_control_connect_error, recv_response,
    send_request,
};

use crate::cli::context::{config_path_or_default, load_config, server_client_from_config};
use crate::cli::device_select::select_device_interactive;
use crate::cli::util::{parse_port_spec, resolve_device_target};

pub async fn port(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
    port: Option<String>,
    protocol: Option<String>,
    remote_port_override: Option<u16>,
    device: Option<String>,
) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;
    let config = load_config(&path, server_url.clone(), false)?;
    let client = server_client_from_config(Some(path.clone()), server_url).await?;
    let response = client.list_devices().await?;

    let port_spec = match port {
        Some(value) => parse_port_spec(&value, remote_port_override)?,
        None => {
            if !io::stdin().is_terminal() {
                return Err(DaemonError::Config(
                    "port required when stdin is not a terminal".to_owned(),
                ));
            }
            let input = Text::new("Port to forward:")
                .with_help_message("e.g. 8080 or 8080:3000")
                .prompt()
                .map_err(|error| DaemonError::Config(error.to_string()))?;
            parse_port_spec(&input, remote_port_override)?
        }
    };

    let protocol = match protocol {
        Some(value) => parse_protocol(&value)?,
        None => PortProtocol::Tcp,
    };

    if protocol != PortProtocol::Tcp {
        return Err(DaemonError::Config(
            "only tcp port forwarding is supported".to_owned(),
        ));
    }

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
        &ControlRequest::OpenPortForward {
            target: target.device_id,
            local_port: port_spec.local_port,
            remote_port: port_spec.remote_port,
            protocol,
        },
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

    eprintln!(
        "Listening on 127.0.0.1:{} on this machine.",
        port_spec.local_port
    );
    eprintln!(
        "Connections will be tunneled to 127.0.0.1:{} on {} (a service must already be listening there).",
        port_spec.remote_port, target.name
    );
    eprintln!("Press Ctrl+C to stop.");

    let (mut read_half, _write_half) = tokio::io::split(stream);
    let mut buf = [0u8; 1];
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        read = read_half.read(&mut buf) => {
            match read {
                Ok(0) | Err(_) => {}
                Ok(_) => {}
            }
        }
    }

    Ok(())
}

fn parse_protocol(value: &str) -> Result<PortProtocol, DaemonError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "tcp" => Ok(PortProtocol::Tcp),
        other => Err(DaemonError::Config(format!(
            "unsupported protocol `{other}`; only tcp is supported"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_protocol_accepts_tcp() {
        assert_eq!(parse_protocol("tcp").unwrap(), PortProtocol::Tcp);
        assert_eq!(parse_protocol("TCP").unwrap(), PortProtocol::Tcp);
    }
}
