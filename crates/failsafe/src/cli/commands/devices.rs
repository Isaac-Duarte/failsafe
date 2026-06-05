use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use failsafe::DaemonError;
use failsafe_core::api::DevicePatchRequest;

use crate::cli::args::DevicesCommand;
use crate::cli::context::server_client_from_config;
use crate::cli::util::{parse_device_id, parse_feature_list};

pub async fn devices(
    command: DevicesCommand,
    server_url: Option<String>,
) -> Result<(), DaemonError> {
    match command {
        DevicesCommand::List { config } => devices_list(config, server_url).await,
        DevicesCommand::Rename { config, id, name } => {
            devices_rename(config, server_url, &id, &name).await
        }
        DevicesCommand::Remove { config, id, yes } => {
            devices_remove(config, server_url, &id, yes).await
        }
        DevicesCommand::Features {
            config,
            id,
            features,
        } => devices_features(config, server_url, &id, &features).await,
    }
}

async fn devices_list(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
) -> Result<(), DaemonError> {
    let client = server_client_from_config(config_path, server_url).await?;
    let response = client.list_devices().await?;

    if response.devices.is_empty() {
        println!("No devices registered.");
        return Ok(());
    }

    println!(
        "{:<20} {:<38} {:<12} {:<20} LAST SEEN",
        "NAME", "DEVICE ID", "STATUS", "FEATURES"
    );
    for device in response.devices {
        let features = device
            .enabled_features
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let status = if device.online { "online" } else { "offline" };
        let last_seen = device.last_seen.unwrap_or_else(|| "—".to_owned());
        println!(
            "{:<20} {:<38} {:<12} {:<20} {}",
            device.name, device.device_id, status, features, last_seen
        );
    }

    Ok(())
}

async fn devices_rename(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
    id: &str,
    name: &str,
) -> Result<(), DaemonError> {
    let client = server_client_from_config(config_path, server_url).await?;
    let device_id = parse_device_id(id)?;

    if name.trim().is_empty() {
        return Err(DaemonError::Config("name cannot be empty".to_owned()));
    }

    client
        .patch_device(
            device_id,
            DevicePatchRequest {
                name: Some(name.trim().to_owned()),
                enabled_features: None,
            },
        )
        .await?;

    println!("Renamed device {device_id} to {name}");
    Ok(())
}

async fn devices_remove(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
    id: &str,
    skip_confirm: bool,
) -> Result<(), DaemonError> {
    let client = server_client_from_config(config_path, server_url).await?;
    let device_id = parse_device_id(id)?;

    if !skip_confirm && io::stdin().is_terminal() {
        print!("Remove device {device_id}? This cannot be undone without re-pairing. [y/N] ");
        io::stdout().flush().map_err(DaemonError::Io)?;

        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .map_err(DaemonError::Io)?;
        let answer = answer.trim().to_ascii_lowercase();
        if answer != "y" && answer != "yes" {
            println!("Cancelled.");
            return Ok(());
        }
    }

    client.delete_device(device_id).await?;
    println!("Removed device {device_id}");
    Ok(())
}

async fn devices_features(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
    id: &str,
    features: &str,
) -> Result<(), DaemonError> {
    let client = server_client_from_config(config_path, server_url).await?;
    let device_id = parse_device_id(id)?;
    let enabled_features = parse_feature_list(features)?;

    client
        .patch_device(
            device_id,
            DevicePatchRequest {
                name: None,
                enabled_features: Some(enabled_features.clone()),
            },
        )
        .await?;

    let feature_list = enabled_features
        .iter()
        .map(|f| f.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    if feature_list.is_empty() {
        println!("Cleared features for device {device_id}");
    } else {
        println!("Updated features for device {device_id}: {feature_list}");
    }
    Ok(())
}
