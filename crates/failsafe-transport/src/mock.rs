use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use failsafe_core::device::DeviceId;
use failsafe_core::message::FeatureMessage;
use tokio::sync::{Mutex, mpsc};

use crate::transport::{Transport, TransportError};

type Router = Arc<Mutex<HashMap<DeviceId, mpsc::Sender<FeatureMessage>>>>;

/// In-memory transport for tests and local development.
pub struct MockTransport {
    local_id: DeviceId,
    inbox: Mutex<mpsc::Receiver<FeatureMessage>>,
    router: Router,
}

impl MockTransport {
    /// Creates two transports that can exchange messages directly.
    pub fn pair() -> (Self, Self) {
        let (tx_a, rx_a) = mpsc::channel(32);
        let (tx_b, rx_b) = mpsc::channel(32);

        let id_a = DeviceId::new();
        let id_b = DeviceId::new();

        let mut routes = HashMap::new();
        routes.insert(id_a, tx_a);
        routes.insert(id_b, tx_b);
        let router = Arc::new(Mutex::new(routes));

        let a = Self {
            local_id: id_a,
            inbox: Mutex::new(rx_a),
            router: router.clone(),
        };
        let b = Self {
            local_id: id_b,
            inbox: Mutex::new(rx_b),
            router,
        };

        (a, b)
    }
}

#[async_trait]
impl Transport for MockTransport {
    fn local_device_id(&self) -> DeviceId {
        self.local_id
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

    #[tokio::test]
    async fn paired_transports_exchange_messages() {
        let (a, b) = MockTransport::pair();

        let message = FeatureMessage::new(
            a.local_device_id(),
            b.local_device_id(),
            FeatureId::Clipboard,
            b"hello",
        );
        a.send(message.clone()).await.unwrap();

        let received = b.recv().await.unwrap();
        assert_eq!(received, message);
    }

    #[tokio::test]
    async fn send_to_unknown_peer_fails() {
        let (a, _b) = MockTransport::pair();

        let err = a
            .send(FeatureMessage::new(
                a.local_device_id(),
                DeviceId::new(),
                FeatureId::Clipboard,
                b"hello",
            ))
            .await
            .unwrap_err();

        assert!(matches!(err, TransportError::PeerNotFound(_)));
    }
}
