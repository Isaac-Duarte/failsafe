use std::path::{Path, PathBuf};

use failsafe::{Config, Credentials, DaemonError, ServerClient};

pub fn config_path_or_default(path: Option<PathBuf>) -> Result<PathBuf, DaemonError> {
    path.or_else(Config::default_path).ok_or_else(|| {
        DaemonError::Config("could not determine config path for this platform".to_owned())
    })
}

pub fn load_config(
    path: &Path,
    server_url: Option<String>,
    create: bool,
) -> Result<Config, DaemonError> {
    let mut config = if create {
        Config::load_or_create(path)?
    } else {
        Config::load(path)?
    };
    config.apply_server_url_override(path, server_url)?;
    Ok(config)
}

pub async fn server_client_from_config(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
) -> Result<ServerClient, DaemonError> {
    let path = config_path_or_default(config_path)?;
    let config = load_config(&path, server_url, true)?;
    let credentials = Credentials::load_or_error()?;
    Ok(ServerClient::new(
        config.server_url.clone(),
        credentials.auth_token,
    ))
}
