use std::path::PathBuf;
use std::process::Command;

use failsafe::DaemonError;
use failsafe_core::api::DeviceInfo;
use failsafe_core::device::DeviceId;

use crate::cli::context::{config_path_or_default, load_config, server_client_from_config};
use crate::cli::util::resolve_device_target;

pub async fn screen_share(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
    device: Option<String>,
) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;
    let config = load_config(&path, server_url.clone(), false)?;
    let client = server_client_from_config(Some(path), server_url).await?;
    let response = client.list_devices().await?;

    let target = match device {
        Some(name) => resolve_device_target(&name, config.device_id, &response.devices)?,
        None => {
            return Err(DaemonError::Config(
                "device name or ID is required for screen share".to_owned(),
            ));
        }
    };

    if !target.online {
        return Err(DaemonError::Config(format!(
            "device {} is offline",
            target.name
        )));
    }

    launch_desktop_viewer(target.device_id, &target.name)
}

fn launch_desktop_viewer(device_id: DeviceId, device_name: &str) -> Result<(), DaemonError> {
    let desktop_binary = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent().map(|dir| {
                dir.join(if cfg!(windows) {
                    "failsafe-desktop.exe"
                } else {
                    "failsafe-desktop"
                })
            })
        })
        .filter(|path| path.exists());

    let Some(binary) = desktop_binary else {
        return Err(DaemonError::Config(
            "failsafe-desktop is not installed; build it with `cargo build -p failsafe-desktop`"
                .to_owned(),
        ));
    };

    let status = Command::new(binary)
        .arg("--screen-share")
        .arg(device_id.to_string())
        .arg("--device-name")
        .arg(device_name)
        .status()
        .map_err(DaemonError::Io)?;

    if status.success() {
        Ok(())
    } else if let Some(code) = status.code() {
        Err(DaemonError::Config(format!(
            "failsafe-desktop exited with status {code}"
        )))
    } else {
        Err(DaemonError::Config(
            "failsafe-desktop exited unexpectedly".to_owned(),
        ))
    }
}

#[allow(dead_code)]
fn select_device_interactive(
    self_id: DeviceId,
    devices: &[DeviceInfo],
) -> Result<DeviceInfo, DaemonError> {
    let _ = (self_id, devices);
    Err(DaemonError::Config(
        "interactive device selection is not implemented for screen share".to_owned(),
    ))
}
