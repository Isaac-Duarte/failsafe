use std::path::PathBuf;
use std::sync::Arc;

use failsafe::{Credentials, Daemon, DaemonError, ServerClient, create_transport_bundle};
use failsafe_core::peer::PeerDirectory;
use tracing::info;

use crate::cli::context::{config_path_or_default, load_config};

pub async fn run(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;
    let config = load_config(&path, server_url, true)?;

    let credentials = Credentials::load_or_error()?;
    let server_client = ServerClient::new(config.server_url.clone(), credentials.auth_token);

    let peers = Arc::new(PeerDirectory::new());
    let bundle = create_transport_bundle(&config).await?;

    if let Some(key) = &bundle.iroh_public_key {
        info!(iroh_public_key = %key, "iroh endpoint ready");
    }

    let mut daemon = Daemon::from_config(path.clone(), config, bundle, peers, Some(server_client))?;
    daemon.run().await
}
