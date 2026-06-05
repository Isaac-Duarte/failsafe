use std::path::PathBuf;

use failsafe::{Credentials, DaemonError};

use crate::cli::context::{config_path_or_default, load_config};

pub fn status(config_path: Option<PathBuf>, server_url: Option<String>) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;

    if !path.exists() {
        return Err(DaemonError::Config(format!(
            "config file not found at {}",
            path.display()
        )));
    }

    let config = load_config(&path, server_url, false)?;

    println!("config: {}", path.display());
    println!("device_id: {}", config.device_id);
    println!("device_name: {}", config.device_name);
    println!("server_url: {}", config.server_url);
    if let Some(credentials_path) = Credentials::default_path() {
        println!(
            "credentials: {}",
            if credentials_path.exists() {
                format!("present at {}", credentials_path.display())
            } else {
                "not found — run `failsafe register`, `failsafe login`, or `failsafe pair --code`"
                    .to_owned()
            }
        );
    }
    println!("transport: iroh");
    println!("enabled_features:");
    for feature in &config.enabled_features {
        println!("  - {feature}");
    }

    Ok(())
}
