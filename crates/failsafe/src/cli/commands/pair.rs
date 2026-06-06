use std::path::{Path, PathBuf};

use failsafe_core::api::DeviceUpsertRequest;
use failsafe_core::peer_address::PeerAddressBook;

use failsafe::{
    Credentials, DaemonError, ServerClient, create_transport_bundle, register_local_device,
};

use crate::cli::context::{config_path_or_default, load_config};
use crate::cli::util::default_hostname;

pub async fn pair(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
    code: Option<String>,
    device_name: Option<String>,
) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;

    match code {
        Some(code) => pair_join(&path, server_url, &code, device_name).await,
        None => pair_host(&path, server_url).await,
    }
}

async fn pair_host(config_path: &Path, server_url: Option<String>) -> Result<(), DaemonError> {
    let config = load_config(config_path, server_url, true)?;
    let credentials_path = Credentials::default_path().ok_or_else(|| {
        DaemonError::Config("could not determine credentials path for this platform".to_owned())
    })?;
    let credentials = Credentials::load_or_error()?;
    let client = ServerClient::new(
        config.server_url.clone(),
        credentials,
        Some(credentials_path),
    );
    let response = client.create_pairing_code().await?;

    println!("Pairing code: {}", response.code);
    println!("Expires at:   {}", response.expires_at);
    println!();
    println!("On the new device, run:");
    println!("  failsafe pair --code {}", response.code);
    Ok(())
}

async fn pair_join(
    config_path: &Path,
    server_url: Option<String>,
    code: &str,
    device_name: Option<String>,
) -> Result<(), DaemonError> {
    let mut config = load_config(config_path, server_url, true)?;
    if let Some(name) = device_name {
        config.device_name = name;
    } else if config.device_name == "my-device" {
        config.device_name = default_hostname();
    }

    let normalized = normalize_pairing_code(code).ok_or_else(|| {
        DaemonError::Config("pairing code must be 6 uppercase alphanumeric characters".to_owned())
    })?;

    let bundle = create_transport_bundle(&config, PeerAddressBook::default()).await?;
    let iroh_public_key = bundle
        .iroh_public_key
        .ok_or_else(|| DaemonError::Config("iroh public key is required".to_owned()))?;

    let response = ServerClient::redeem_pairing_code(
        &config.server_url,
        &normalized,
        Some(DeviceUpsertRequest {
            device_id: config.device_id,
            name: config.device_name.clone(),
            iroh_public_key: iroh_public_key.clone(),
            enabled_features: config.enabled_features.clone(),
        }),
    )
    .await?;

    let credentials_path = Credentials::default_path().ok_or_else(|| {
        DaemonError::Config("could not determine credentials path for this platform".to_owned())
    })?;
    let token = response
        .token
        .ok_or_else(|| DaemonError::Config("pairing response missing auth token".to_owned()))?;
    let refresh_token = response
        .refresh_token
        .ok_or_else(|| DaemonError::Config("pairing response missing refresh token".to_owned()))?;

    let credentials = Credentials {
        auth_token: token,
        refresh_token: Some(refresh_token),
    };
    credentials.save(&credentials_path)?;
    config.save(config_path)?;

    register_local_device(&config, credentials).await?;

    println!("Paired successfully.");
    println!("Device ID:   {}", config.device_id);
    println!("Device name: {}", config.device_name);
    println!();
    println!("Start syncing with:");
    println!("  failsafe run");
    Ok(())
}

pub(crate) fn normalize_pairing_code(code: &str) -> Option<String> {
    let normalized = code.trim().to_uppercase();
    if normalized.len() != 6 {
        return None;
    }

    if !normalized
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
    {
        return None;
    }

    Some(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_pairing_code_accepts_case_insensitive_input() {
        assert_eq!(normalize_pairing_code("a3k9z1").as_deref(), Some("A3K9Z1"));
        assert!(normalize_pairing_code("too-short").is_none());
    }
}
