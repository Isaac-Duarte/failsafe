use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use failsafe_core::message::FeatureMessage;
use failsafe_core::outbound::{OutboundMessage, OutboundPublisher, PublishError};
use failsafe_core::peer::PeerDirectory;

use crate::transport::Transport;

/// Routes outbound feature events to connected peers using directory policy.
pub struct MessageRouter {
    transport: Arc<dyn Transport>,
    peers: Arc<PeerDirectory>,
}

impl MessageRouter {
    pub fn new(transport: Arc<dyn Transport>, peers: Arc<PeerDirectory>) -> Self {
        Self { transport, peers }
    }

    pub fn into_publisher(
        transport: Arc<dyn Transport>,
        peers: Arc<PeerDirectory>,
    ) -> Arc<dyn OutboundPublisher> {
        Arc::new(Self::new(transport, peers))
    }
}

#[async_trait]
impl OutboundPublisher for MessageRouter {
    async fn publish(&self, outbound: OutboundMessage) -> Result<(), PublishError> {
        let local_id = self.transport.local_device_id();
        let candidates = self.peers.recipients_for(outbound.feature).await;
        if candidates.is_empty() {
            return Ok(());
        }

        let connected: HashSet<_> = self.transport.connected_peers().await.into_iter().collect();
        let recipients: Vec<_> = candidates
            .into_iter()
            .filter(|peer| connected.contains(peer))
            .collect();

        if recipients.is_empty() {
            return Ok(());
        }

        let mut failures = Vec::new();

        for peer in recipients {
            let message = FeatureMessage::new(
                local_id,
                peer,
                outbound.feature,
                outbound.payload.clone(),
            );

            if let Err(error) = self.transport.send(message).await {
                failures.push(format!("{peer}: {error}"));
            }
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(PublishError::Failed(failures.join("; ")))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use failsafe_core::feature::FeatureId;

    use super::*;
    use crate::mock::MockTransport;
    use crate::transport::Transport;

    #[tokio::test]
    async fn publishes_to_configured_connected_peer() {
        let (local_transport, peer_transport) = MockTransport::pair();
        let peer_id = peer_transport.local_device_id();

        let peers = Arc::new(PeerDirectory::new());
        peers.replace_peers([peer_id]).await;

        let publisher =
            MessageRouter::into_publisher(Arc::new(local_transport), peers);

        publisher
            .publish(OutboundMessage::new(FeatureId::Clipboard, b"hello"))
            .await
            .unwrap();

        let received = peer_transport.recv().await.unwrap();
        assert_eq!(received.feature, FeatureId::Clipboard);
        assert_eq!(received.payload, b"hello");
    }

    #[tokio::test]
    async fn skips_peers_with_feature_disabled() {
        let (local_transport, peer_transport) = MockTransport::pair();
        let peer_id = peer_transport.local_device_id();

        let peers = Arc::new(PeerDirectory::new());
        peers.replace_peers([peer_id]).await;
        peers
            .set_feature_enabled(peer_id, FeatureId::Clipboard, false)
            .await;

        let publisher =
            MessageRouter::into_publisher(Arc::new(local_transport), peers);

        publisher
            .publish(OutboundMessage::new(FeatureId::Clipboard, b"hello"))
            .await
            .unwrap();

        assert!(peer_transport.try_recv().await.unwrap().is_none());
    }
}
