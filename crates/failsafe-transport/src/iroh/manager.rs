use std::sync::Arc;
use std::time::Duration;

use failsafe_core::device::DeviceId;
use failsafe_core::message::FeatureMessage;
use iroh::Endpoint;
use iroh::endpoint::Connection;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::iroh::address::SharedAddressState;
use crate::iroh::config::FAILSAFE_ALPN;
use crate::iroh::protocol::resolve_device;
use crate::iroh::stream::{
    SharedLanAcceptor, SharedPortAcceptor, SharedShellAcceptor, handle_incoming_bi_stream,
};
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
    local_device_id: DeviceId,
    shell_acceptor: SharedShellAcceptor,
    port_acceptor: SharedPortAcceptor,
    lan_acceptor: SharedLanAcceptor,
) -> ManagerCommand {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let task = tokio::spawn(async move {
        if let Err(error) = run_dial_manager(
            endpoint,
            pool,
            inbox,
            address_state,
            local_device_id,
            shell_acceptor,
            port_acceptor,
            lan_acceptor,
            shutdown_rx,
        )
        .await
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
    local_device_id: DeviceId,
    shell_acceptor: SharedShellAcceptor,
    port_acceptor: SharedPortAcceptor,
    lan_acceptor: SharedLanAcceptor,
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
                    local_device_id,
                    shell_acceptor.clone(),
                    port_acceptor.clone(),
                    lan_acceptor.clone(),
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
    local_device_id: DeviceId,
    shell_acceptor: SharedShellAcceptor,
    port_acceptor: SharedPortAcceptor,
    lan_acceptor: SharedLanAcceptor,
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
        try_dial_one(
            device,
            &address,
            endpoint.as_ref(),
            pool.clone(),
            address_state,
            inbox.clone(),
            local_device_id,
            shell_acceptor.clone(),
            port_acceptor.clone(),
            lan_acceptor.clone(),
        )
        .await;
    }
}

async fn try_dial_one(
    device: DeviceId,
    address: &str,
    endpoint: &Endpoint,
    pool: Arc<ConnectionPool>,
    address_state: &SharedAddressState,
    inbox: mpsc::Sender<FeatureMessage>,
    local_device_id: DeviceId,
    shell_acceptor: SharedShellAcceptor,
    port_acceptor: SharedPortAcceptor,
    lan_acceptor: SharedLanAcceptor,
) {
    if pool.get(device).await.is_some() {
        return;
    }

    let Ok(endpoint_addr) = crate::iroh::transport::parse_endpoint_addr(address) else {
        warn!(%device, %address, "skipping peer with invalid iroh address");
        return;
    };

    if endpoint_addr.id == endpoint.id() {
        warn!(
            %device,
            %address,
            "skipping peer with same iroh endpoint as this device; remove the stale device from the server or re-pair it"
        );
        return;
    }

    let connection = match endpoint.connect(endpoint_addr, FAILSAFE_ALPN).await {
        Ok(connection) => connection,
        Err(error) => {
            debug!(%device, "failed to dial peer: {error}");
            return;
        }
    };

    if let Err(error) = register_dialed_connection(
        &connection,
        pool,
        address_state,
        inbox,
        local_device_id,
        shell_acceptor,
        port_acceptor,
        lan_acceptor,
    )
    .await
    {
        warn!(%device, "failed to register outbound connection: {error}");
        return;
    }

    debug!(%device, "connected to peer");
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
    local_device_id: DeviceId,
    shell_acceptor: SharedShellAcceptor,
    port_acceptor: SharedPortAcceptor,
    lan_acceptor: SharedLanAcceptor,
) -> Result<(), TransportError> {
    let device = register_outbound_connection(connection, address_state)?;
    pool.insert(device, connection.clone()).await;
    spawn_stream_handler(
        connection.clone(),
        device,
        pool,
        inbox,
        local_device_id,
        shell_acceptor,
        port_acceptor,
        lan_acceptor,
    );
    Ok(())
}

fn spawn_stream_handler(
    connection: Connection,
    device: DeviceId,
    pool: Arc<ConnectionPool>,
    inbox: mpsc::Sender<FeatureMessage>,
    local_device_id: DeviceId,
    shell_acceptor: SharedShellAcceptor,
    port_acceptor: SharedPortAcceptor,
    lan_acceptor: SharedLanAcceptor,
) {
    tokio::spawn(async move {
        loop {
            match connection.accept_bi().await {
                Ok((send, recv)) => {
                    let inbox = inbox.clone();
                    let shell_acceptor = shell_acceptor.clone();
                    let port_acceptor = port_acceptor.clone();
                    let lan_acceptor = lan_acceptor.clone();
                    tokio::spawn(async move {
                        handle_incoming_bi_stream(
                            send,
                            recv,
                            device,
                            local_device_id,
                            inbox,
                            port_acceptor,
                            shell_acceptor,
                            lan_acceptor,
                        )
                        .await;
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
