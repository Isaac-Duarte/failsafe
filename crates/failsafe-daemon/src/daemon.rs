use std::collections::HashSet;
use std::sync::Arc;

use failsafe_clipboard::feature::ClipboardFeature;
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_core::peer::PeerDirectory;
use failsafe_core::registry::FeatureRegistry;
use failsafe_transport::router::MessageRouter;
use failsafe_transport::transport::Transport;
use tracing::info;

use crate::config::Config;
use crate::error::DaemonError;

pub struct Daemon {
    transport: Arc<dyn Transport>,
    peers: Arc<PeerDirectory>,
    registry: FeatureRegistry,
}

pub struct DaemonBuilder {
    transport: Option<Arc<dyn Transport>>,
    peers: Arc<PeerDirectory>,
    enabled_features: HashSet<FeatureId>,
}

impl DaemonBuilder {
    pub fn new() -> Self {
        Self {
            transport: None,
            peers: Arc::new(PeerDirectory::new()),
            enabled_features: HashSet::new(),
        }
    }

    pub fn transport(mut self, transport: Arc<dyn Transport>) -> Self {
        self.transport = Some(transport);
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

    pub fn build(self) -> Result<Daemon, DaemonError> {
        let transport = self
            .transport
            .ok_or_else(|| DaemonError::Config("transport is required".to_owned()))?;

        let publisher = MessageRouter::into_publisher(transport.clone(), self.peers.clone());
        let mut registry = FeatureRegistry::new();

        if self.enabled_features.contains(&FeatureId::Clipboard) {
            registry.register(Box::new(ClipboardFeature::new(publisher)))?;
            registry.enable(FeatureId::Clipboard)?;
        }

        Ok(Daemon {
            transport,
            peers: self.peers,
            registry,
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
        transport: Arc<dyn Transport>,
        peers: Arc<PeerDirectory>,
    ) -> Result<Self, DaemonError> {
        Daemon::builder()
            .transport(transport)
            .peers(peers)
            .enable_features(config.enabled_feature_set())
            .build()
    }

    pub fn device_id(&self) -> DeviceId {
        self.transport.local_device_id()
    }

    pub fn peers(&self) -> &Arc<PeerDirectory> {
        &self.peers
    }

    pub async fn apply_config(&self, config: &Config) {
        self.peers.replace_peers(config.peers.clone()).await;
    }

    pub async fn run(&mut self) -> Result<(), DaemonError> {
        info!(device_id = %self.device_id(), "starting daemon");

        self.registry.start_enabled().await?;

        loop {
            tokio::select! {
                result = self.transport.recv() => {
                    let message = result?;
                    self.registry.dispatch(message).await?;
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

pub async fn create_transport(
    config: &Config,
    network: Option<Arc<failsafe_transport::mock::MockNetwork>>,
) -> Result<Arc<dyn Transport>, DaemonError> {
    match config.transport {
        crate::config::TransportKind::Mock => {
            let network = network.unwrap_or_else(failsafe_transport::mock::MockNetwork::new);
            let transport = network.connect_with_id(config.device_id).await;
            Ok(Arc::new(transport))
        }
        crate::config::TransportKind::Iroh => {
            Err(DaemonError::TransportUnavailable("iroh".to_owned()))
        }
    }
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
        let transport = Arc::new(network.connect_with_id(config.device_id).await);
        let peers = Arc::new(PeerDirectory::new());

        let daemon = Daemon::from_config(&config, transport, peers).unwrap();
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
