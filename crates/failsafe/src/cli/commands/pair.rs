use std::path::{Path, PathBuf};

use failsafe::{Credentials, DaemonError, ServerClient, register_local_device};

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
    let credentials = Credentials::load_or_error()?;
    let client = ServerClient::new(config.server_url.clone(), credentials.auth_token);
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

    let response = ServerClient::redeem_pairing_code(&config.server_url, &normalized).await?;

    let credentials_path = Credentials::default_path().ok_or_else(|| {
        DaemonError::Config("could not determine credentials path for this platform".to_owned())
    })?;
    Credentials {
        auth_token: response.token.clone(),
    }
    .save(&credentials_path)?;
    config.save(config_path)?;

    register_local_device(&config, response.token).await?;

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
