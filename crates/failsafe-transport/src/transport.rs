use std::time::Duration;

use async_trait::async_trait;
use failsafe_core::device::DeviceId;
use failsafe_core::message::FeatureMessage;

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("peer `{0}` not found")]
    PeerNotFound(DeviceId),

    #[error("transport disconnected")]
    Disconnected,

    #[error("transport codec error: {0}")]
    Codec(String),
}

/// Device-to-device message delivery.
#[async_trait]
pub trait Transport: Send + Sync {
    fn local_device_id(&self) -> DeviceId;

    async fn send(&self, message: FeatureMessage) -> Result<(), TransportError>;

    /// Peers with an active transport session.
    async fn connected_peers(&self) -> Vec<DeviceId>;

    async fn try_recv(&self) -> Result<Option<FeatureMessage>, TransportError>;

    async fn recv(&self) -> Result<FeatureMessage, TransportError> {
        loop {
            if let Some(message) = self.try_recv().await? {
                return Ok(message);
            }

            // Fallback for transports that only implement try_recv. Prefer overriding
            // recv() to block on the inbox channel directly (see IrohTransport).
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}
