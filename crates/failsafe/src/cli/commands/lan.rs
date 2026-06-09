use std::path::PathBuf;

use failsafe::DaemonError;
use failsafe_core::control::connect_control;
use failsafe_core::feature::FeatureSpec;
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

pub fn setup() -> Result<(), DaemonError> {
    #[cfg(target_os = "linux")]
    {
        let exe = std::env::current_exe().map_err(|error| {
            DaemonError::Config(format!("failed to resolve failsafe binary path: {error}"))
        })?;
        let exe = exe.canonicalize().unwrap_or(exe);

        eprintln!("This will grant the failsafe binary permission to manage virtual network interfaces without sudo.");
        eprintln!("You will be prompted for your password once.");

        let status = std::process::Command::new("sudo")
            .arg("setcap")
            .arg("cap_net_admin+ep")
            .arg(&exe)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .map_err(|error| DaemonError::Config(format!("failed to run sudo setcap: {error}")))?;

        if status.success() {
            eprintln!("Virtual LAN capabilities installed for {}", exe.display());
            eprintln!("Restart the daemon if it is already running.");
        } else {
            return Err(DaemonError::Config(
                "setcap failed; virtual LAN will fall back to sudo when enabled".to_owned(),
            ));
        }
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        eprintln!("macOS does not support Linux-style capabilities (setcap).");
        eprintln!("Virtual LAN will prompt for your password via sudo when it starts.");
        eprintln!("Run `failsafe run` from Terminal.app so sudo can read your password.");
        eprintln!("sudo caches credentials for a few minutes between prompts.");
        return Ok(());
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(DaemonError::Config(
            "virtual LAN setup is only documented for Linux and macOS".to_owned(),
        ))
    }
}

pub fn tun_helper(socket: std::path::PathBuf, ip: String) -> Result<(), DaemonError> {
    failsafe_lan::run_tun_helper(&socket, &ip).map_err(|error| DaemonError::Config(error.to_string()))
}
