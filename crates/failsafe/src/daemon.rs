use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use failsafe_clipboard::feature::ClipboardFeature;
use failsafe_core::api::DeviceUpsertRequest;
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_core::peer::PeerDirectory;
use failsafe_core::peer_address::PeerAddressBook;
use failsafe_core::registry::FeatureRegistry;
use failsafe_transport::peer_updater::PeerAddressUpdater;
use failsafe_transport::router::MessageRouter;
use failsafe_transport::transport::Transport;
use tracing::info;

use crate::config::Config;
use crate::error::DaemonError;
use crate::server::ServerClient;
use crate::sync::apply_server_devices;

pub struct TransportBundle {
    pub transport: Arc<dyn Transport>,
    pub peer_updater: Arc<dyn PeerAddressUpdater>,
    pub iroh_public_key: Option<String>,
}

pub struct Daemon {
    transport: Arc<dyn Transport>,
    peer_updater: Arc<dyn PeerAddressUpdater>,
    peers: Arc<PeerDirectory>,
    registry: FeatureRegistry,
    server_client: Option<ServerClient>,
    local_features: HashSet<FeatureId>,
    device_name: String,
    iroh_public_key: Option<String>,
}

pub struct DaemonBuilder {
    transport: Option<Arc<dyn Transport>>,
    peer_updater: Option<Arc<dyn PeerAddressUpdater>>,
    peers: Arc<PeerDirectory>,
    enabled_features: HashSet<FeatureId>,
    server_client: Option<ServerClient>,
    device_name: String,
    iroh_public_key: Option<String>,
}

impl DaemonBuilder {
    pub fn new() -> Self {
        Self {
            transport: None,
            peer_updater: None,
            peers: Arc::new(PeerDirectory::new()),
            enabled_features: HashSet::new(),
            server_client: None,
            device_name: "my-device".to_owned(),
            iroh_public_key: None,
        }
    }

    pub fn transport(mut self, transport: Arc<dyn Transport>) -> Self {
        self.transport = Some(transport);
        self
    }

    pub fn peer_updater(mut self, peer_updater: Arc<dyn PeerAddressUpdater>) -> Self {
        self.peer_updater = Some(peer_updater);
        self
    }

    pub fn peers(mut self, peers: Arc<PeerDirectory>) -> Self {
        self.peers = peers;
        self
    }

    pub fn enable_feature(mut self, feature: FeatureId) -> Self {
        self.enabled_features.insert(feature);
        self
    }

    pub fn enable_features(mut self, features: impl IntoIterator<Item = FeatureId>) -> Self {
        self.enabled_features.extend(features);
        self
    }

    pub fn server_client(mut self, client: ServerClient) -> Self {
        self.server_client = Some(client);
        self
    }

    pub fn device_name(mut self, name: String) -> Self {
        self.device_name = name;
        self
    }

    pub fn iroh_public_key(mut self, key: Option<String>) -> Self {
        self.iroh_public_key = key;
        self
    }

    pub fn build(self) -> Result<Daemon, DaemonError> {
        let transport = self
            .transport
            .ok_or_else(|| DaemonError::Config("transport is required".to_owned()))?;
        let peer_updater = self
            .peer_updater
            .ok_or_else(|| DaemonError::Config("peer updater is required".to_owned()))?;

        let publisher = MessageRouter::into_publisher(transport.clone(), self.peers.clone());
        let mut registry = FeatureRegistry::new();

        if self.enabled_features.contains(&FeatureId::Clipboard) {
            registry.register(Box::new(ClipboardFeature::new(publisher)))?;
            registry.enable(FeatureId::Clipboard)?;
        }

        Ok(Daemon {
            transport,
            peer_updater,
            peers: self.peers,
            registry,
            server_client: self.server_client,
            local_features: self.enabled_features,
            device_name: self.device_name,
            iroh_public_key: self.iroh_public_key,
        })
    }
}

impl Default for DaemonBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Daemon {
    pub fn builder() -> DaemonBuilder {
        DaemonBuilder::new()
    }

    pub fn from_config(
        config: &Config,
        bundle: TransportBundle,
        peers: Arc<PeerDirectory>,
        server_client: Option<ServerClient>,
    ) -> Result<Self, DaemonError> {
        let mut builder = Daemon::builder()
            .transport(bundle.transport)
            .peer_updater(bundle.peer_updater)
            .peers(peers)
            .enable_features(config.enabled_feature_set())
            .device_name(config.device_name.clone())
            .iroh_public_key(bundle.iroh_public_key);

        if let Some(client) = server_client {
            builder = builder.server_client(client);
        }

        builder.build()
    }

    pub fn device_id(&self) -> DeviceId {
        self.transport.local_device_id()
    }

    pub fn peers(&self) -> &Arc<PeerDirectory> {
        &self.peers
    }

    pub async fn register_with_server(&self) -> Result<(), DaemonError> {
        let client = self.server_client.as_ref().ok_or_else(|| {
            DaemonError::Config("credentials are required; pair this device first".to_owned())
        })?;

        let iroh_public_key = self.iroh_public_key.clone().ok_or_else(|| {
            DaemonError::Config("iroh public key is required; set transport = \"iroh\"".to_owned())
        })?;

        client
            .upsert_device(DeviceUpsertRequest {
                device_id: self.device_id(),
                name: self.device_name.clone(),
                iroh_public_key,
                enabled_features: self.local_features.iter().copied().collect(),
            })
            .await
    }

    pub async fn sync_from_server(&self) -> Result<(), DaemonError> {
        let client = self.server_client.as_ref().ok_or_else(|| {
            DaemonError::Config("credentials are required; pair this device first".to_owned())
        })?;

        let response = client.list_devices().await?;
        apply_server_devices(
            self.device_id(),
            &self.local_features,
            &response.devices,
            &self.peers,
            self.peer_updater.as_ref(),
        )
        .await;

        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), DaemonError> {
        if self.server_client.is_none() {
            return Err(DaemonError::Config(
                "server_url and credentials are required; run `failsafe register`, `failsafe login`, or `failsafe pair --code`".to_owned(),
            ));
        }

        info!(device_id = %self.device_id(), "starting daemon");

        self.register_with_server().await?;
        self.sync_from_server().await?;

        self.registry.start_enabled().await?;

        let mut sync_interval = tokio::time::interval(Duration::from_secs(30));
        sync_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                result = self.transport.recv() => {
                    let message = result?;
                    self.registry.dispatch(message).await?;
                }
                _ = sync_interval.tick() => {
                    if let Err(error) = self.sync_from_server().await {
                        tracing::warn!("server sync failed: {error}");
                    }
                }
                result = tokio::signal::ctrl_c() => {
                    result.map_err(DaemonError::Io)?;
                    info!("shutdown signal received");
                    break;
                }
            }
        }

        self.shutdown().await
    }

    async fn shutdown(&mut self) -> Result<(), DaemonError> {
        let enabled: Vec<_> = self.registry.enabled_features().collect();
        for feature in enabled {
            self.registry.disable(feature).await;
        }
        info!("daemon stopped");
        Ok(())
    }
}

pub async fn create_transport_bundle(
    config: &Config,
    network: Option<Arc<failsafe_transport::mock::MockNetwork>>,
) -> Result<TransportBundle, DaemonError> {
    match config.transport {
        crate::config::TransportKind::Mock => {
            let network = network.unwrap_or_else(failsafe_transport::mock::MockNetwork::new);
            let transport = Arc::new(network.connect_with_id(config.device_id).await);
            let peer_updater = transport.clone();
            Ok(TransportBundle {
                transport,
                peer_updater,
                iroh_public_key: None,
            })
        }
        crate::config::TransportKind::Iroh => {
            let secret_key_path = Config::default_secret_key_path().ok_or_else(|| {
                DaemonError::Config("could not determine iroh secret key path".to_owned())
            })?;

            let transport = Arc::new(
                failsafe_transport::iroh::IrohTransport::start(
                    failsafe_transport::iroh::IrohConfig {
                        device_id: config.device_id,
                        secret_key_path,
                        address_book: PeerAddressBook::default(),
                    },
                )
                .await?,
            );
            let iroh_public_key = transport.public_key_hex();
            let peer_updater = transport.clone();

            Ok(TransportBundle {
                transport,
                peer_updater,
                iroh_public_key: Some(iroh_public_key),
            })
        }
    }
}

pub async fn create_transport(
    config: &Config,
    network: Option<Arc<failsafe_transport::mock::MockNetwork>>,
) -> Result<Arc<dyn Transport>, DaemonError> {
    Ok(create_transport_bundle(config, network).await?.transport)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use failsafe_core::peer::PeerDirectory;
    use failsafe_transport::mock::MockNetwork;

    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn builds_from_config() {
        let network = MockNetwork::new();
        let config = Config::new(DeviceId::new());
        let bundle = create_transport_bundle(&config, Some(network.clone()))
            .await
            .unwrap();
        let peers = Arc::new(PeerDirectory::new());

        let daemon = Daemon::from_config(&config, bundle, peers, None).unwrap();
        assert_eq!(daemon.device_id(), config.device_id);
    }

    #[tokio::test]
    async fn create_transport_uses_configured_device_id() {
        let config = Config::new(DeviceId::new());
        let transport = create_transport(&config, Some(MockNetwork::new()))
            .await
            .unwrap();

        assert_eq!(transport.local_device_id(), config.device_id);
    }
}
