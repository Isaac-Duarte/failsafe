use std::path::PathBuf;

use failsafe::{Credentials, DaemonError, ServerClient, register_local_device};
use tracing::info;

use crate::cli::context::{config_path_or_default, load_config};

pub async fn authenticate(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
    email: String,
    password: String,
    register: bool,
) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;
    let config = load_config(&path, server_url, true)?;

    let response = if register {
        ServerClient::register(&config.server_url, &email, &password).await?
    } else {
        ServerClient::login(&config.server_url, &email, &password).await?
    };

    let credentials_path = Credentials::default_path().ok_or_else(|| {
        DaemonError::Config("could not determine credentials path for this platform".to_owned())
    })?;
    let token = response.token.ok_or_else(|| {
        DaemonError::Config(
            "server requires two-factor authentication; use the web UI or pass --totp when supported"
                .to_owned(),
        )
    })?;
    let refresh_token = response
        .refresh_token
        .ok_or_else(|| DaemonError::Config("server response missing refresh token".to_owned()))?;

    let credentials = Credentials {
        auth_token: token,
        refresh_token: Some(refresh_token),
    };
    credentials.save(&credentials_path)?;

    register_local_device(&config, credentials).await?;

    info!(
        credentials = %credentials_path.display(),
        "saved authentication credentials"
    );

    if register {
        println!("Account created and logged in.");
    } else {
        println!("Logged in.");
    }
    println!("Credentials saved to {}", credentials_path.display());
    println!();
    println!("Start syncing with:");
    println!("  failsafe run");
    Ok(())
}
