use std::sync::Arc;
use std::time::Duration;

use failsafe_core::device::DeviceId;
use failsafe_core::message::FeatureMessage;
use iroh::Endpoint;
use iroh::endpoint::Connection;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::codec;
use crate::iroh::address::SharedAddressState;
use crate::iroh::config::FAILSAFE_ALPN;
use crate::iroh::protocol::resolve_device;
use crate::transport::TransportError;

#[derive(Debug)]
pub struct ConnectionPool {
    connections: tokio::sync::Mutex<std::collections::HashMap<DeviceId, Connection>>,
}

impl ConnectionPool {
    pub fn new() -> Self {
        Self {
            connections: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub async fn insert(&self, device: DeviceId, connection: Connection) {
        self.connections.lock().await.insert(device, connection);
    }

    pub async fn remove(&self, device: DeviceId) {
        self.connections.lock().await.remove(&device);
    }

    pub async fn get(&self, device: DeviceId) -> Option<Connection> {
        self.connections.lock().await.get(&device).cloned()
    }

    pub async fn connected_peers(&self) -> Vec<DeviceId> {
        self.connections.lock().await.keys().copied().collect()
    }
}

pub struct ManagerCommand {
    shutdown: watch::Sender<bool>,
    _task: JoinHandle<()>,
}

impl ManagerCommand {
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(true);
    }
}

pub fn spawn_dial_manager(
    endpoint: Endpoint,
    pool: Arc<ConnectionPool>,
    inbox: mpsc::Sender<FeatureMessage>,
    address_state: SharedAddressState,
) -> ManagerCommand {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(async move {
        if let Err(error) =
            run_dial_manager(endpoint, pool, inbox, address_state, shutdown_rx).await
        {
            warn!("iroh dial manager exited with error: {error}");
        }
    });

    ManagerCommand {
        shutdown: shutdown_tx,
        _task: task,
    }
}

async fn run_dial_manager(
    endpoint: Endpoint,
    pool: Arc<ConnectionPool>,
    inbox: mpsc::Sender<FeatureMessage>,
    address_state: SharedAddressState,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), TransportError> {
    let endpoint = Arc::new(endpoint);
    let mut dial_interval = tokio::time::interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            _ = dial_interval.tick() => {
                dial_peers(
                    endpoint.clone(),
                    pool.clone(),
                    &address_state,
                    inbox.clone(),
                ).await;
            }
        }
    }

    Ok(())
}

async fn dial_peers(
    endpoint: Arc<Endpoint>,
    pool: Arc<ConnectionPool>,
    address_state: &SharedAddressState,
    inbox: mpsc::Sender<FeatureMessage>,
) {
    let peers_to_dial: Vec<(DeviceId, String)> = match address_state.read() {
        Ok(state) => state
            .book
            .iter()
            .map(|(device, address)| (device, address.to_owned()))
            .collect(),
        Err(error) => {
            warn!("address state lock poisoned: {error}");
            return;
        }
    };

    for (device, address) in peers_to_dial {
        if pool.get(device).await.is_some() {
            continue;
        }

        let Ok(endpoint_addr) = crate::iroh::transport::parse_endpoint_addr(&address) else {
            warn!(%device, %address, "skipping peer with invalid iroh address");
            continue;
        };

        match endpoint.connect(endpoint_addr, FAILSAFE_ALPN).await {
            Ok(connection) => {
                if let Err(error) = register_dialed_connection(
                    &connection,
                    pool.clone(),
                    address_state,
                    inbox.clone(),
                )
                .await
                {
                    warn!(%device, "failed to register outbound connection: {error}");
                    continue;
                }
                debug!(%device, "connected to peer");
            }
            Err(error) => {
                debug!(%device, "failed to dial peer: {error}");
            }
        }
    }
}

pub(crate) fn register_outbound_connection(
    connection: &Connection,
    address_state: &SharedAddressState,
) -> Result<DeviceId, TransportError> {
    resolve_device(connection, address_state)
}

async fn register_dialed_connection(
    connection: &Connection,
    pool: Arc<ConnectionPool>,
    address_state: &SharedAddressState,
    inbox: mpsc::Sender<FeatureMessage>,
) -> Result<(), TransportError> {
    let device = register_outbound_connection(connection, address_state)?;
    pool.insert(device, connection.clone()).await;
    spawn_stream_handler(connection.clone(), device, pool, inbox);
    Ok(())
}

fn spawn_stream_handler(
    connection: Connection,
    device: DeviceId,
    pool: Arc<ConnectionPool>,
    inbox: mpsc::Sender<FeatureMessage>,
) {
    tokio::spawn(async move {
        loop {
            match connection.accept_bi().await {
                Ok((_send, mut recv)) => {
                    let inbox = inbox.clone();
                    tokio::spawn(async move {
                        match recv.read_to_end(16 * 1024 * 1024).await {
                            Ok(bytes) => match codec::decode(&bytes) {
                                Ok(message) => {
                                    if inbox.send(message).await.is_err() {
                                        debug!("inbox closed while delivering message");
                                    }
                                }
                                Err(error) => warn!("failed to decode inbound frame: {error}"),
                            },
                            Err(error) => warn!("failed to read inbound stream: {error}"),
                        }
                    });
                }
                Err(error) => {
                    debug!(%device, "connection stream accept ended: {error}");
                    pool.remove(device).await;
                    break;
                }
            }
        }
    });
}
