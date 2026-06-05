use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use failsafe_core::device::DeviceId;
use failsafe_core::message::FeatureMessage;
use failsafe_core::peer_address::PeerAddressBook;
use iroh::{Endpoint, EndpointAddr, PublicKey, SecretKey, endpoint::presets};
use tokio::sync::{Mutex, mpsc};
use tracing::info;

use crate::codec;
use crate::iroh::address::{update_address_state, AddressState, SharedAddressState};
use crate::iroh::config::{FAILSAFE_ALPN, IrohConfig};
use crate::iroh::manager::{ConnectionPool, ManagerCommand, spawn_manager};
use crate::peer_updater::PeerAddressUpdater;
use crate::transport::{Transport, TransportError};

pub struct IrohTransport {
    device_id: DeviceId,
    endpoint: Endpoint,
    pool: Arc<ConnectionPool>,
    inbox: Mutex<mpsc::Receiver<FeatureMessage>>,
    manager: ManagerCommand,
    address_state: SharedAddressState,
}

impl IrohTransport {
    pub async fn start(config: IrohConfig) -> Result<Self, TransportError> {
        let secret_key = load_or_create_secret_key(&config.secret_key_path)?;
        let endpoint = Endpoint::builder(presets::N0)
            .secret_key(secret_key)
            .alpns(vec![FAILSAFE_ALPN.to_vec()])
            .bind()
            .await
            .map_err(|error| TransportError::Codec(error.to_string()))?;

        info!(
            public_key = %endpoint.id(),
            "iroh endpoint ready"
        );

        let (inbox_tx, inbox_rx) = mpsc::channel(256);
        let pool = Arc::new(ConnectionPool::new());
        let address_state = Arc::new(RwLock::new(
            AddressState::from_book(config.address_book.clone())?,
        ));

        let manager = spawn_manager(
            endpoint.clone(),
            pool.clone(),
            inbox_tx,
            address_state.clone(),
        );

        Ok(Self {
            device_id: config.device_id,
            endpoint,
            pool,
            inbox: Mutex::new(inbox_rx),
            manager,
            address_state,
        })
    }

    pub fn public_key(&self) -> PublicKey {
        self.endpoint.id()
    }

    pub fn public_key_hex(&self) -> String {
        self.endpoint.id().to_string()
    }

    pub fn endpoint_addr(&self) -> EndpointAddr {
        self.endpoint.addr()
    }

    pub fn update_peers(&self, book: PeerAddressBook) -> Result<(), TransportError> {
        update_address_state(&self.address_state, book)
    }
}

impl PeerAddressUpdater for IrohTransport {
    fn update_peer_addresses(&self, book: PeerAddressBook) {
        if let Err(error) = self.update_peers(book) {
            tracing::warn!("failed to update iroh peer addresses: {error}");
        }
    }
}

#[async_trait]
impl Transport for IrohTransport {
    fn local_device_id(&self) -> DeviceId {
        self.device_id
    }

    async fn send(&self, message: FeatureMessage) -> Result<(), TransportError> {
        let connection = self
            .pool
            .get(message.to)
            .await
            .ok_or(TransportError::PeerNotFound(message.to))?;

        let frame = codec::encode(&message)?;
        let (mut send, _recv) = connection
            .open_bi()
            .await
            .map_err(|error| TransportError::Codec(error.to_string()))?;

        send.write_all(&frame)
            .await
            .map_err(|error| TransportError::Codec(error.to_string()))?;
        send.finish()
            .map_err(|error| TransportError::Codec(error.to_string()))?;

        Ok(())
    }

    async fn connected_peers(&self) -> Vec<DeviceId> {
        self.pool.connected_peers().await
    }

    async fn try_recv(&self) -> Result<Option<FeatureMessage>, TransportError> {
        let mut inbox = self.inbox.lock().await;
        match inbox.try_recv() {
            Ok(message) => Ok(Some(message)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => Err(TransportError::Disconnected),
        }
    }
}

impl Drop for IrohTransport {
    fn drop(&mut self) {
        self.manager.shutdown();
    }
}

pub(crate) fn parse_endpoint_addr(address: &str) -> Result<EndpointAddr, TransportError> {
    let public_key = PublicKey::from_str(address)
        .map_err(|error| TransportError::Codec(format!("invalid iroh public key: {error}")))?;
    Ok(EndpointAddr::new(public_key))
}

fn load_or_create_secret_key(path: &Path) -> Result<SecretKey, TransportError> {
    if path.exists() {
        let bytes =
            std::fs::read(path).map_err(|error| TransportError::Codec(error.to_string()))?;
        if bytes.len() != 32 {
            return Err(TransportError::Codec(format!(
                "secret key at {} must be 32 bytes",
                path.display()
            )));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        return Ok(SecretKey::from_bytes(&key));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| TransportError::Codec(error.to_string()))?;
    }

    let secret_key = SecretKey::generate();
    std::fs::write(path, secret_key.to_bytes())
        .map_err(|error| TransportError::Codec(error.to_string()))?;
    Ok(secret_key)
}

pub(crate) fn build_reverse_lookup(
    book: &PeerAddressBook,
) -> Result<HashMap<String, DeviceId>, TransportError> {
    let mut reverse = HashMap::new();
    for (device, address) in book.iter() {
        let endpoint_addr = parse_endpoint_addr(address)?;
        reverse.insert(endpoint_addr.id.to_string(), device);
    }
    Ok(reverse)
}
