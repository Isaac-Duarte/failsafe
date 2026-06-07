use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use failsafe_core::device::DeviceId;
use failsafe_core::message::FeatureMessage;
use tokio::sync::{Mutex, mpsc};

use failsafe_core::peer_address::PeerAddressBook;

use crate::peer_updater::PeerAddressUpdater;
use crate::transport::{Transport, TransportError};

type Router = Arc<Mutex<HashMap<DeviceId, mpsc::Sender<FeatureMessage>>>>;

/// Shared in-memory network for connecting multiple mock transports.
pub struct MockNetwork {
    router: Router,
}

impl MockNetwork {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            router: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn connect(self: &Arc<Self>) -> MockTransport {
        self.connect_with_id(DeviceId::new()).await
    }

    pub async fn connect_with_id(self: &Arc<Self>, local_id: DeviceId) -> MockTransport {
        let (tx, rx) = mpsc::channel(32);
        self.router.lock().await.insert(local_id, tx);

        MockTransport {
            local_id,
            inbox: Mutex::new(rx),
            router: self.router.clone(),
        }
    }
}

/// In-memory transport for unit tests (`test-util` feature).
pub struct MockTransport {
    local_id: DeviceId,
    inbox: Mutex<mpsc::Receiver<FeatureMessage>>,
    router: Router,
}

impl MockTransport {
    /// Creates two transports on an isolated network.
    pub async fn pair() -> (Self, Self) {
        let network = MockNetwork::new();
        let a = network.connect().await;
        let b = network.connect().await;
        (a, b)
    }
}

impl PeerAddressUpdater for MockTransport {
    fn update_peer_addresses(&self, _book: PeerAddressBook) {}
}

#[async_trait]
impl Transport for MockTransport {
    fn local_device_id(&self) -> DeviceId {
        self.local_id
    }

    async fn connected_peers(&self) -> Vec<DeviceId> {
        let router = self.router.lock().await;
        router
            .keys()
            .copied()
            .filter(|peer| *peer != self.local_id)
            .collect()
    }

    async fn send(&self, message: FeatureMessage) -> Result<(), TransportError> {
        let router = self.router.lock().await;
        let peer = router
            .get(&message.to)
            .ok_or(TransportError::PeerNotFound(message.to))?;
        peer.send(message)
            .await
            .map_err(|_| TransportError::Disconnected)
    }

    async fn try_recv(&self) -> Result<Option<FeatureMessage>, TransportError> {
        let mut inbox = self.inbox.lock().await;
        match inbox.try_recv() {
            Ok(message) => Ok(Some(message)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => Err(TransportError::Disconnected),
        }
    }

    async fn recv(&self) -> Result<FeatureMessage, TransportError> {
        let mut inbox = self.inbox.lock().await;
        inbox.recv().await.ok_or(TransportError::Disconnected)
    }
}

#[cfg(test)]
mod tests {
    use failsafe_core::feature::FeatureId;

    use super::*;
    use crate::transport::Transport;

    #[tokio::test]
    async fn paired_transports_exchange_messages() {
        let (a, b) = MockTransport::pair().await;

        let message = FeatureMessage::new(
            a.local_device_id(),
            b.local_device_id(),
            FeatureId::from_static("clipboard"),
            b"hello",
        );
        a.send(message.clone()).await.unwrap();

        let received = b.recv().await.unwrap();
        assert_eq!(received, message);
    }

    #[tokio::test]
    async fn network_supports_multiple_peers() {
        let network = MockNetwork::new();
        let a = network.connect().await;
        let b = network.connect().await;
        let c = network.connect().await;

        let message = FeatureMessage::new(
            a.local_device_id(),
            c.local_device_id(),
            FeatureId::from_static("clipboard"),
            b"hello",
        );
        a.send(message.clone()).await.unwrap();

        assert!(b.try_recv().await.unwrap().is_none());
        assert_eq!(c.recv().await.unwrap(), message);
    }

    #[tokio::test]
    async fn send_to_unknown_peer_fails() {
        let (a, _b) = MockTransport::pair().await;

        let err = a
            .send(FeatureMessage::new(
                a.local_device_id(),
                DeviceId::new(),
                FeatureId::from_static("clipboard"),
                b"hello",
            ))
            .await
            .unwrap_err();

        assert!(matches!(err, TransportError::PeerNotFound(_)));
    }
}
