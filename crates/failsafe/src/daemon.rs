use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use failsafe_clipboard::feature::ClipboardFeature;
use failsafe_clipboard::limits::ClipboardLimits;
use failsafe_send::{parse_ack, SendCoordinator, SendFeature};
use failsafe_transport::blobs::MockBlobTransfer;
use failsafe_core::api::DeviceUpsertRequest;
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_core::peer::PeerDirectory;
use failsafe_core::peer_address::PeerAddressBook;
use failsafe_core::registry::FeatureRegistry;
use failsafe_transport::blobs::BlobTransfer;
use failsafe_transport::iroh::{PortSession, ShellSession};
use failsafe_transport::peer_updater::PeerAddressUpdater;
use failsafe_transport::router::MessageRouter;
use failsafe_transport::transport::Transport;
use tokio::sync::{RwLock, mpsc};
use tracing::info;

use crate::config::Config;
use crate::control_server::ControlServer;
use crate::error::DaemonError;
use failsafe_port::{handle_incoming_port, start_port_acceptor, stop_port_acceptor};
use crate::server::ServerClient;
use crate::shell_service::{handle_incoming_shell, start_shell_acceptor, stop_shell_acceptor};
use crate::sync::{apply_self_from_server, apply_server_devices};

pub struct TransportBundle {
    pub transport: Arc<dyn Transport>,
    pub peer_updater: Arc<dyn PeerAddressUpdater>,
    pub blob_transfer: Option<Arc<dyn BlobTransfer>>,
    pub iroh_public_key: Option<String>,
    pub iroh: Option<Arc<failsafe_transport::iroh::IrohTransport>>,
}

pub struct Daemon {
    transport: Arc<dyn Transport>,
    peer_updater: Arc<dyn PeerAddressUpdater>,
    peers: Arc<PeerDirectory>,
    registry: FeatureRegistry,
    send_coordinator: Arc<SendCoordinator>,
    server_client: Option<ServerClient>,
    local_features: HashSet<FeatureId>,
    device_name: String,
    iroh_public_key: Option<String>,
    iroh: Option<Arc<failsafe_transport::iroh::IrohTransport>>,
    blob_transfer: Arc<dyn BlobTransfer>,
    send_limits: ClipboardLimits,
    shell_sessions: Option<mpsc::Receiver<ShellSession>>,
    port_sessions: Option<mpsc::Receiver<PortSession>>,
    config_path: Option<PathBuf>,
    config: Option<Config>,
}

pub struct DaemonBuilder {
    transport: Option<Arc<dyn Transport>>,
    peer_updater: Option<Arc<dyn PeerAddressUpdater>>,
    peers: Arc<PeerDirectory>,
    enabled_features: HashSet<FeatureId>,
    server_client: Option<ServerClient>,
    device_name: String,
    iroh_public_key: Option<String>,
    blob_transfer: Option<Arc<dyn BlobTransfer>>,
    clipboard_limits: ClipboardLimits,
    iroh: Option<Arc<failsafe_transport::iroh::IrohTransport>>,
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
            blob_transfer: None,
            clipboard_limits: ClipboardLimits::default(),
            iroh: None,
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

    pub fn blob_transfer(mut self, blob_transfer: Option<Arc<dyn BlobTransfer>>) -> Self {
        self.blob_transfer = blob_transfer;
        self
    }

    pub fn clipboard_limits(mut self, limits: ClipboardLimits) -> Self {
        self.clipboard_limits = limits;
        self
    }

    pub fn iroh(mut self, iroh: Option<Arc<failsafe_transport::iroh::IrohTransport>>) -> Self {
        self.iroh = iroh;
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

        registry.register(Box::new(ClipboardFeature::new_with_limits(
            publisher,
            self.blob_transfer.clone(),
            self.clipboard_limits,
        )))?;

        let blob_transfer = self
            .blob_transfer
            .clone()
            .unwrap_or_else(|| Arc::new(MockBlobTransfer::new()));
        let send_coordinator = SendCoordinator::new();
        registry.register(Box::new(SendFeature::new(
            blob_transfer.clone(),
            self.clipboard_limits,
            transport.clone(),
            send_coordinator.clone(),
        )))?;

        if self.enabled_features.contains(&FeatureId::Clipboard) {
            registry.enable(FeatureId::Clipboard)?;
        }

        if self.enabled_features.contains(&FeatureId::FileSend) {
            registry.enable(FeatureId::FileSend)?;
        }

        Ok(Daemon {
            transport,
            peer_updater,
            peers: self.peers,
            registry,
            send_coordinator,
            server_client: self.server_client,
            local_features: self.enabled_features,
            device_name: self.device_name,
            iroh_public_key: self.iroh_public_key,
            iroh: self.iroh,
            blob_transfer,
            send_limits: self.clipboard_limits,
            shell_sessions: None,
            port_sessions: None,
            config_path: None,
            config: None,
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
        config_path: PathBuf,
        config: Config,
        bundle: TransportBundle,
        peers: Arc<PeerDirectory>,
        server_client: Option<ServerClient>,
    ) -> Result<Self, DaemonError> {
        let mut builder = Daemon::builder()
            .transport(bundle.transport)
            .peer_updater(bundle.peer_updater)
            .blob_transfer(bundle.blob_transfer)
            .clipboard_limits(config.clipboard_limits())
            .peers(peers)
            .enable_features(config.enabled_feature_set())
            .device_name(config.device_name.clone())
            .iroh_public_key(bundle.iroh_public_key)
            .iroh(bundle.iroh.clone());

        if let Some(client) = server_client {
            builder = builder.server_client(client);
        }

        let mut daemon = builder.build()?;
        daemon.config_path = Some(config_path);
        daemon.config = Some(config);
        Ok(daemon)
    }

    pub fn device_id(&self) -> DeviceId {
        self.transport.local_device_id()
    }

    pub fn peers(&self) -> &Arc<PeerDirectory> {
        &self.peers
    }

    pub async fn register_transport_with_server(&self) -> Result<(), DaemonError> {
        let client = self.server_client.as_ref().ok_or_else(|| {
            DaemonError::Config("credentials are required; pair this device first".to_owned())
        })?;

        let iroh_public_key = self
            .iroh_public_key
            .clone()
            .ok_or_else(|| DaemonError::Config("iroh public key is required".to_owned()))?;

        // Name and features are included for first-time device creation only.
        // The server ignores them on subsequent transport updates.
        client
            .upsert_device(DeviceUpsertRequest {
                device_id: self.device_id(),
                name: self.device_name.clone(),
                iroh_public_key,
                enabled_features: self.local_features.iter().copied().collect(),
            })
            .await
    }

    pub async fn heartbeat_with_server(&self) -> Result<(), DaemonError> {
        let client = self.server_client.as_ref().ok_or_else(|| {
            DaemonError::Config("credentials are required; pair this device first".to_owned())
        })?;

        client.heartbeat_device(self.device_id()).await
    }

    pub async fn sync_from_server(&mut self, start_new_features: bool) -> Result<(), DaemonError> {
        let client = self.server_client.as_ref().ok_or_else(|| {
            DaemonError::Config("credentials are required; pair this device first".to_owned())
        })?;

        let response = client.list_devices().await?;
        let self_id = self.device_id();
        let config_path = self.config_path.clone();

        if let (Some(config_path), Some(config)) = (config_path.as_ref(), self.config.as_mut()) {
            apply_self_from_server(
                self_id,
                &response.devices,
                &mut self.device_name,
                &mut self.local_features,
                config,
                config_path,
                &mut self.registry,
                start_new_features,
            )
            .await?;
        }

        apply_server_devices(
            self_id,
            &self.local_features,
            &response.devices,
            &self.peers,
            self.peer_updater.as_ref(),
        )
        .await;

        Ok(())
    }

    async fn ensure_shell_service(&mut self) {
        let shell_enabled = self.local_features.contains(&FeatureId::Shell);

        if shell_enabled && self.shell_sessions.is_none() {
            if let Some(iroh) = self.iroh.clone() {
                self.shell_sessions = Some(start_shell_acceptor(iroh).await);
            }
        } else if !shell_enabled && self.shell_sessions.is_some() {
            if let Some(iroh) = &self.iroh {
                stop_shell_acceptor(iroh).await;
            }
            self.shell_sessions = None;
        }
    }

    async fn ensure_port_service(&mut self) {
        let port_enabled = self.local_features.contains(&FeatureId::PortForward);

        if port_enabled && self.port_sessions.is_none() {
            if let Some(iroh) = self.iroh.clone() {
                self.port_sessions = Some(start_port_acceptor(iroh).await);
            }
        } else if !port_enabled && self.port_sessions.is_some() {
            if let Some(iroh) = &self.iroh {
                stop_port_acceptor(iroh).await;
            }
            self.port_sessions = None;
        }
    }

    pub async fn run(&mut self) -> Result<(), DaemonError> {
        if self.server_client.is_none() {
            return Err(DaemonError::Config(
                "server_url and credentials are required; run `failsafe register`, `failsafe login`, or `failsafe pair --code`".to_owned(),
            ));
        }

        info!(device_id = %self.device_id(), "starting daemon");

        self.sync_from_server(false).await?;
        self.register_transport_with_server().await?;
        self.sync_from_server(false).await?;

        self.registry.start_enabled().await?;
        self.ensure_shell_service().await;
        self.ensure_port_service().await;

        let shared_features = Arc::new(RwLock::new(self.local_features.clone()));
        let control_server: Option<Arc<ControlServer>> = self
            .iroh
            .clone()
            .map(|iroh| {
                ControlServer::new(
                    iroh,
                    self.transport.clone(),
                    self.blob_transfer.clone(),
                    self.device_name.clone(),
                    self.send_limits,
                    self.send_coordinator.clone(),
                    shared_features.clone(),
                    self.peers.clone(),
                )
            })
            .transpose()?
            .map(Arc::new);
        let control_listener = match control_server.as_ref() {
            Some(server) => Some(server.bind().await?),
            None => None,
        };

        let mut sync_interval = tokio::time::interval(Duration::from_secs(30));
        sync_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        let (inbound_tx, mut inbound_rx) = mpsc::channel::<failsafe_core::message::FeatureMessage>(256);
        let transport_reader = self.transport.clone();
        tokio::spawn(async move {
            loop {
                match transport_reader.recv().await {
                    Ok(message) => {
                        if inbound_tx.send(message).await.is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        tracing::warn!("transport reader stopped: {error}");
                        break;
                    }
                }
            }
        });

        loop {
            tokio::select! {
                message = inbound_rx.recv() => {
                    let Some(message) = message else {
                        return Err(DaemonError::Config(
                            "inbound message channel closed".to_owned(),
                        ));
                    };
                    if message.feature == FeatureId::FileSend {
                        if let Some(ack) = parse_ack(&message.payload) {
                            self.send_coordinator.complete_ack(ack).await;
                            continue;
                        }
                    }
                    self.registry.dispatch(message).await?;
                }
                session = async {
                    match self.shell_sessions.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Some(session) = session {
                        tokio::spawn(handle_incoming_shell(session));
                    }
                }
                port_session = async {
                    match self.port_sessions.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Some(session) = port_session {
                        tokio::spawn(handle_incoming_port(session));
                    }
                }
                accepted = async {
                    match control_listener.as_ref() {
                        Some(listener) => listener.accept().await.ok(),
                        None => std::future::pending().await,
                    }
                } => {
                    if let (Some(server), Some((stream, _))) = (control_server.as_ref(), accepted) {
                        let server = Arc::clone(server);
                        tokio::spawn(async move {
                            server.handle_connection(stream).await;
                        });
                    }
                }
                _ = sync_interval.tick() => {
                    if let Err(error) = self.heartbeat_with_server().await {
                        if matches!(error, DaemonError::DeviceRemoved) {
                            return Err(error);
                        }
                        tracing::warn!("server heartbeat failed: {error}");
                    }
                    if let Err(error) = self.sync_from_server(true).await {
                        tracing::warn!("server sync failed: {error}");
                    } else {
                        *shared_features.write().await = self.local_features.clone();
                        self.ensure_shell_service().await;
                        self.ensure_port_service().await;
                    }
                }
                result = tokio::signal::ctrl_c() => {
                    result.map_err(DaemonError::Io)?;
                    info!("shutdown signal received");
                    break;
                }
            }
        }

        if let Some(iroh) = &self.iroh {
            stop_shell_acceptor(iroh).await;
            stop_port_acceptor(iroh).await;
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

pub async fn register_local_device(
    config: &Config,
    credentials: crate::credentials::Credentials,
) -> Result<(), DaemonError> {
    let bundle = create_transport_bundle(config, PeerAddressBook::default()).await?;
    let iroh_public_key = bundle
        .iroh_public_key
        .ok_or_else(|| DaemonError::Config("iroh public key is required".to_owned()))?;

    let client = ServerClient::new(config.server_url.clone(), credentials, None);
    client
        .upsert_device(DeviceUpsertRequest {
            device_id: config.device_id,
            name: config.device_name.clone(),
            iroh_public_key,
            enabled_features: config.enabled_features.clone(),
        })
        .await
}

pub async fn create_transport_bundle(
    config: &Config,
    address_book: PeerAddressBook,
) -> Result<TransportBundle, DaemonError> {
    let secret_key_path = Config::default_secret_key_path().ok_or_else(|| {
        DaemonError::Config("could not determine iroh secret key path".to_owned())
    })?;
    let blob_store_path = config
        .resolved_blob_store_path()
        .ok_or_else(|| DaemonError::Config("could not determine blob store path".to_owned()))?;

    let iroh = Arc::new(
        failsafe_transport::iroh::IrohTransport::start(failsafe_transport::iroh::IrohConfig {
            device_id: config.device_id,
            secret_key_path,
            blob_store_path,
            address_book,
        })
        .await?,
    );
    let iroh_public_key = iroh.public_key_hex();
    let peer_updater = iroh.clone();
    let blob_transfer = Some(iroh.blob_transfer());
    let transport: Arc<dyn Transport> = iroh.clone();

    Ok(TransportBundle {
        transport,
        peer_updater,
        blob_transfer,
        iroh_public_key: Some(iroh_public_key),
        iroh: Some(iroh),
    })
}

pub async fn create_transport(config: &Config) -> Result<Arc<dyn Transport>, DaemonError> {
    Ok(create_transport_bundle(config, PeerAddressBook::default())
        .await?
        .transport)
}

#[cfg(test)]
pub async fn create_test_transport_bundle(
    config: &Config,
    network: Arc<failsafe_transport::mock::MockNetwork>,
) -> TransportBundle {
    let transport = Arc::new(network.connect_with_id(config.device_id).await);
    let peer_updater = transport.clone();
    TransportBundle {
        transport,
        peer_updater,
        blob_transfer: None,
        iroh_public_key: None,
        iroh: None,
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
        let config = Config::new(DeviceId::new());
        let device_id = config.device_id;
        let bundle = create_test_transport_bundle(&config, MockNetwork::new()).await;
        let peers = Arc::new(PeerDirectory::new());

        let daemon =
            Daemon::from_config(Config::default_path().unwrap(), config, bundle, peers, None)
                .unwrap();
        assert_eq!(daemon.device_id(), device_id);
    }

    #[tokio::test]
    async fn create_test_transport_uses_configured_device_id() {
        let config = Config::new(DeviceId::new());
        let bundle = create_test_transport_bundle(&config, MockNetwork::new()).await;

        assert_eq!(bundle.transport.local_device_id(), config.device_id);
    }
}
