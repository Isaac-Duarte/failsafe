use std::sync::Arc;

use failsafe_core::device::DeviceId;
use failsafe_core::message::FeatureMessage;
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::iroh::SharedAddressState;
use crate::iroh::manager::{ConnectionPool, register_outbound_connection};
use crate::iroh::stream::{
    SharedScreenAcceptor, SharedShellAcceptor, handle_incoming_bi_stream,
};
use crate::transport::TransportError;

#[derive(Debug, Clone)]
pub struct FailsafeProtocol {
    pool: Arc<ConnectionPool>,
    inbox: mpsc::Sender<FeatureMessage>,
    address_state: SharedAddressState,
    shell_acceptor: SharedShellAcceptor,
    screen_acceptor: SharedScreenAcceptor,
}

impl FailsafeProtocol {
    pub fn new(
        pool: Arc<ConnectionPool>,
        inbox: mpsc::Sender<FeatureMessage>,
        address_state: SharedAddressState,
        shell_acceptor: SharedShellAcceptor,
        screen_acceptor: SharedScreenAcceptor,
    ) -> Self {
        Self {
            pool,
            inbox,
            address_state,
            shell_acceptor,
            screen_acceptor,
        }
    }
}

impl ProtocolHandler for FailsafeProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let device = match register_outbound_connection(&connection, &self.address_state) {
            Ok(device) => device,
            Err(error) => {
                warn!("failed to register inbound failsafe connection: {error}");
                return Ok(());
            }
        };

        self.pool.insert(device, connection.clone()).await;

        debug!(%device, "accepted failsafe protocol connection");

        loop {
            match connection.accept_bi().await {
                Ok((send, recv)) => {
                    let inbox = self.inbox.clone();
                    let shell_acceptor = self.shell_acceptor.clone();
                    let screen_acceptor = self.screen_acceptor.clone();
                    tokio::spawn(async move {
                        handle_incoming_bi_stream(
                            send,
                            recv,
                            device,
                            inbox,
                            shell_acceptor,
                            screen_acceptor,
                        )
                        .await;
                    });
                }
                Err(error) => {
                    debug!(%device, "failsafe connection stream accept ended: {error}");
                    self.pool.remove(device).await;
                    break;
                }
            }
        }

        Ok(())
    }
}

pub(crate) fn resolve_device(
    connection: &Connection,
    address_state: &SharedAddressState,
) -> Result<DeviceId, TransportError> {
    let remote_id = connection.remote_id().to_string();
    let state = address_state
        .read()
        .map_err(|error| TransportError::Codec(format!("address state lock poisoned: {error}")))?;
    state
        .reverse_lookup
        .get(&remote_id)
        .copied()
        .ok_or_else(|| {
            TransportError::Codec(format!(
                "unknown remote endpoint {remote_id}; waiting for server peer sync"
            ))
        })
}
