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
    Credentials {
        auth_token: response.token.clone(),
    }
    .save(&credentials_path)?;

    register_local_device(&config, response.token).await?;

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
