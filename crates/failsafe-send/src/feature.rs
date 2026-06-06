use std::sync::Arc;

use async_trait::async_trait;
use failsafe_clipboard::limits::ClipboardLimits;
use failsafe_core::device::DeviceId;
use failsafe_core::feature::{Feature, FeatureError, FeatureId};
use failsafe_core::message::FeatureMessage;
use failsafe_transport::blobs::BlobTransfer;
use failsafe_transport::transport::{Transport, TransportError};
use tracing::info;

use crate::coordinator::SendCoordinator;
use crate::inbound::save_received_files;
use crate::notify::notify_files_received;
use crate::payload::{decode_envelope, encode_envelope, SendAck, SendEnvelope};

pub struct SendFeature {
    blob_transfer: Arc<dyn BlobTransfer>,
    limits: ClipboardLimits,
    transport: Arc<dyn Transport>,
    coordinator: Arc<SendCoordinator>,
}

impl SendFeature {
    pub fn new(
        blob_transfer: Arc<dyn BlobTransfer>,
        limits: ClipboardLimits,
        transport: Arc<dyn Transport>,
        coordinator: Arc<SendCoordinator>,
    ) -> Self {
        Self {
            blob_transfer,
            limits,
            transport,
            coordinator,
        }
    }

    async fn handle_transfer(
        &self,
        from: DeviceId,
        payload: crate::payload::SendPayload,
    ) -> Result<(), FeatureError> {
        let files = self
            .blob_transfer
            .fetch_collection_files(
                from,
                &failsafe_transport::blobs::BlobHash::from(payload.collection_hash.as_str()),
            )
            .await
            .map_err(|error| FeatureError::Failed(FeatureId::FileSend, error.to_string()))?;

        self.limits
            .validate_files(&files)
            .map_err(|error| FeatureError::Failed(FeatureId::FileSend, error))?;

        let paths = save_received_files(&payload.sender_name, payload.transfer_id, &files)
            .await
            .map_err(|error| FeatureError::Failed(FeatureId::FileSend, error))?;

        let destination = paths
            .first()
            .and_then(|path| path.parent())
            .map(|path| path.to_path_buf())
            .unwrap_or_default();

        notify_files_received(&payload.sender_name, paths.len(), &destination);

        info!(
            %from,
            transfer_id = %payload.transfer_id,
            count = paths.len(),
            "received file send"
        );

        self.send_ack(
            from,
            SendAck {
                transfer_id: payload.transfer_id,
                ok: true,
                error: None,
            },
        )
        .await?;

        Ok(())
    }

    async fn handle_ack(&self, ack: SendAck) -> Result<(), FeatureError> {
        let result = if ack.ok {
            Ok(())
        } else {
            Err(ack
                .error
                .unwrap_or_else(|| "receiver reported failure".to_owned()))
        };
        self.coordinator.complete(ack.transfer_id, result).await;
        Ok(())
    }

    async fn send_ack(&self, to: DeviceId, ack: SendAck) -> Result<(), FeatureError> {
        let local_id = self.transport.local_device_id();
        let envelope = SendEnvelope::Ack(ack);
        let message = FeatureMessage::new(local_id, to, FeatureId::FileSend, encode_envelope(&envelope));
        self.transport
            .send(message)
            .await
            .map_err(transport_error_to_feature_error)
    }
}

#[async_trait]
impl Feature for SendFeature {
    fn id(&self) -> FeatureId {
        FeatureId::FileSend
    }

    async fn start(&mut self) -> Result<(), FeatureError> {
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), FeatureError> {
        Ok(())
    }

    async fn handle_message(&mut self, message: FeatureMessage) -> Result<(), FeatureError> {
        let envelope = decode_envelope(&message.payload)?;
        match envelope {
            SendEnvelope::Transfer(payload) => self.handle_transfer(message.from, payload).await,
            SendEnvelope::Ack(ack) => self.handle_ack(ack).await,
        }
    }
}

fn transport_error_to_feature_error(error: TransportError) -> FeatureError {
    FeatureError::Failed(FeatureId::FileSend, error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use failsafe_core::feature::Feature;
    use failsafe_core::message::FeatureMessage;
    use failsafe_transport::blobs::MockBlobTransfer;
    use failsafe_transport::mock::MockTransport;
    use uuid::Uuid;

    use super::*;
    use crate::payload::{FileEntry, SendPayload, SEND_PAYLOAD_VERSION};

    #[tokio::test]
    async fn receives_transfer_and_sends_ack() {
        let (local_transport, peer_transport) = MockTransport::pair().await;
        let local_id = local_transport.local_device_id();
        let peer_id = peer_transport.local_device_id();

        let blob_transfer = Arc::new(MockBlobTransfer::new());
        let hash = blob_transfer
            .store_files(vec![("hello.txt".to_owned(), b"hello".to_vec())])
            .await
            .unwrap();

        let coordinator = SendCoordinator::new();
        let peer_transport: Arc<dyn Transport> = Arc::new(peer_transport);
        let mut feature = SendFeature::new(
            blob_transfer,
            ClipboardLimits::default(),
            peer_transport,
            coordinator,
        );

        let payload = SendPayload {
            version: SEND_PAYLOAD_VERSION,
            transfer_id: Uuid::new_v4(),
            sender_name: "sender".to_owned(),
            collection_hash: hash.as_str().to_owned(),
            entries: vec![FileEntry {
                name: "hello.txt".to_owned(),
                size: 5,
            }],
        };

        feature
            .handle_message(FeatureMessage::new(
                local_id,
                peer_id,
                FeatureId::FileSend,
                encode_envelope(&SendEnvelope::Transfer(payload)),
            ))
            .await
            .unwrap();

        let ack_message = local_transport.try_recv().await.unwrap().unwrap();
        assert_eq!(ack_message.feature, FeatureId::FileSend);
        let ack = decode_envelope(&ack_message.payload).unwrap();
        assert!(matches!(ack, SendEnvelope::Ack(SendAck { ok: true, .. })));
    }
}
