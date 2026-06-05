use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use failsafe_core::device::DeviceId;
use failsafe_core::message::FeatureMessage;
use failsafe_core::peer_address::PeerAddressBook;
use iroh::Endpoint;
use iroh::endpoint::Connection;
use tokio::sync::{Mutex, mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::codec;
use crate::iroh::config::FAILSAFE_ALPN;
use crate::transport::TransportError;

pub struct ConnectionPool {
    connections: Mutex<HashMap<DeviceId, Connection>>,
}

impl ConnectionPool {
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
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

pub fn spawn_manager(
    endpoint: Endpoint,
    pool: Arc<ConnectionPool>,
    inbox: mpsc::Sender<FeatureMessage>,
    address_book: PeerAddressBook,
    reverse_lookup: HashMap<String, DeviceId>,
) -> ManagerCommand {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(async move {
        if let Err(error) = run_manager(
            endpoint,
            pool,
            inbox,
            address_book,
            reverse_lookup,
            shutdown_rx,
        )
        .await
        {
            warn!("iroh manager exited with error: {error}");
        }
    });

    ManagerCommand {
        shutdown: shutdown_tx,
        _task: task,
    }
}

async fn run_manager(
    endpoint: Endpoint,
    pool: Arc<ConnectionPool>,
    inbox: mpsc::Sender<FeatureMessage>,
    address_book: PeerAddressBook,
    reverse_lookup: HashMap<String, DeviceId>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), TransportError> {
    let endpoint = Arc::new(endpoint);
    let reverse_lookup = Arc::new(reverse_lookup);

    let accept_endpoint = endpoint.clone();
    let accept_pool = pool.clone();
    let accept_lookup = reverse_lookup.clone();
    let accept_inbox = inbox.clone();
    let mut accept_shutdown = shutdown.clone();

    let accept_task = tokio::spawn(async move {
        loop {
            if *accept_shutdown.borrow() {
                break;
            }

            let incoming = tokio::select! {
                incoming = accept_endpoint.accept() => incoming,
                _ = accept_shutdown.changed() => break,
            };

            let Some(incoming) = incoming else {
                continue;
            };

            let accept_pool = accept_pool.clone();
            let accept_lookup = accept_lookup.clone();
            let accept_inbox = accept_inbox.clone();

            tokio::spawn(async move {
                match incoming.await {
                    Ok(connection) => {
                        if let Err(error) = register_connection(
                            &connection,
                            accept_pool.clone(),
                            accept_lookup.as_ref(),
                            accept_inbox.clone(),
                        )
                        .await
                        {
                            warn!("failed to register inbound connection: {error}");
                        }
                    }
                    Err(error) => warn!("failed to accept iroh connection: {error}"),
                }
            });
        }
    });

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
                    &address_book,
                    reverse_lookup.clone(),
                    inbox.clone(),
                ).await;
            }
        }
    }

    accept_task.abort();
    Ok(())
}

async fn dial_peers(
    endpoint: Arc<Endpoint>,
    pool: Arc<ConnectionPool>,
    address_book: &PeerAddressBook,
    reverse_lookup: Arc<HashMap<String, DeviceId>>,
    inbox: mpsc::Sender<FeatureMessage>,
) {
    for (device, address) in address_book.iter() {
        if pool.get(device).await.is_some() {
            continue;
        }

        let Ok(endpoint_addr) = crate::iroh::transport::parse_endpoint_addr(address) else {
            warn!(%device, address, "skipping peer with invalid iroh address");
            continue;
        };

        match endpoint.connect(endpoint_addr, FAILSAFE_ALPN).await {
            Ok(connection) => {
                if let Err(error) = register_connection(
                    &connection,
                    pool.clone(),
                    reverse_lookup.as_ref(),
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

async fn register_connection(
    connection: &Connection,
    pool: Arc<ConnectionPool>,
    reverse_lookup: &HashMap<String, DeviceId>,
    inbox: mpsc::Sender<FeatureMessage>,
) -> Result<(), TransportError> {
    let remote_id = connection.remote_id().to_string();
    let device = reverse_lookup.get(&remote_id).copied().ok_or_else(|| {
        TransportError::Codec(format!(
            "unknown remote endpoint {remote_id}; add it to peer_addresses"
        ))
    })?;

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
