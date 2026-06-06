use std::sync::Arc;

use async_trait::async_trait;
use failsafe_clipboard::limits::ClipboardLimits;
use failsafe_core::device::DeviceId;
use failsafe_core::feature::{Feature, FeatureError, FeatureId};
use failsafe_core::message::FeatureMessage;
use failsafe_transport::blobs::{BlobHash, BlobTransfer};
use failsafe_transport::transport::{Transport, TransportError};
use tracing::info;
use uuid::Uuid;

use crate::coordinator::SendCoordinator;
use crate::inbound::receive_dir;
use crate::notify::notify_files_received;
use crate::payload::{decode_envelope, encode_envelope, SendAck, SendEnvelope};
use crate::resume::receive_state_from_payload;
use crate::transfer_state::{
    load_receive_state, remove_receive_state, save_receive_state, ReceiveStage,
    ReceiveTransferState,
};

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

    pub async fn resume_receive(
        &self,
        blob_transfer: Arc<dyn BlobTransfer>,
        mut state: ReceiveTransferState,
    ) -> Result<(), String> {
        self.run_receive(blob_transfer, &mut state).await
    }

    pub async fn acknowledge_completed_receive(
        &self,
        sender: DeviceId,
        transfer_id: Uuid,
    ) -> Result<(), String> {
        self.send_ack(
            sender,
            SendAck {
                transfer_id,
                ok: true,
                error: None,
            },
        )
        .await
        .map_err(|error| error.to_string())
    }

    async fn handle_transfer(
        &self,
        from: DeviceId,
        payload: crate::payload::SendPayload,
    ) -> Result<(), FeatureError> {
        let mut state = match load_receive_state(payload.transfer_id).await {
            Ok(saved) => saved,
            Err(_) => receive_state_from_payload(from, &payload),
        };

        if state.stage == ReceiveStage::Complete {
            return self
                .send_ack(
                    from,
                    SendAck {
                        transfer_id: payload.transfer_id,
                        ok: true,
                        error: None,
                    },
                )
                .await;
        }

        state.sender = from;
        state.sender_name = payload.sender_name.clone();
        state.collection_hash = payload.collection_hash.clone();
        state.entries = payload.entries.clone();
        state.bytes_total = payload.entries.iter().map(|entry| entry.size).sum();

        self.run_receive(self.blob_transfer.clone(), &mut state)
            .await
            .map_err(|error| FeatureError::Failed(FeatureId::FileSend, error))?;

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

    async fn run_receive(
        &self,
        blob_transfer: Arc<dyn BlobTransfer>,
        state: &mut ReceiveTransferState,
    ) -> Result<(), String> {
        self.limits.validate_entries(
            &state
                .entries
                .iter()
                .map(|entry| (entry.name.clone(), entry.size))
                .collect::<Vec<_>>(),
        )?;

        state.stage = ReceiveStage::Downloading;
        save_receive_state(state).await?;

        let root = BlobHash::from(state.collection_hash.as_str());
        blob_transfer
            .download_collection(
                state.sender,
                &root,
                state.bytes_total,
                &mut |progress| {
                    state.bytes_done = progress.bytes_done;
                },
            )
            .await
            .map_err(|error| error.to_string())?;

        state.stage = ReceiveStage::Exporting;
        save_receive_state(state).await?;

        let receive_dir = state
            .receive_dir
            .clone()
            .or_else(|| receive_dir(&state.sender_name, state.transfer_id))
            .ok_or_else(|| "downloads directory unavailable".to_owned())?;
        state.receive_dir = Some(receive_dir.clone());

        let paths = blob_transfer
            .export_collection(&root, &receive_dir, &mut |progress| {
                state.bytes_done = progress.bytes_done;
            })
            .await
            .map_err(|error| error.to_string())?;

        state.stage = ReceiveStage::Complete;
        state.bytes_done = state.bytes_total;
        save_receive_state(state).await?;
        remove_receive_state(state.transfer_id).await?;

        notify_files_received(&state.sender_name, paths.len(), &receive_dir);

        info!(
            sender = %state.sender,
            transfer_id = %state.transfer_id,
            count = paths.len(),
            "received file send"
        );

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
        let feature = SendFeature {
            blob_transfer: self.blob_transfer.clone(),
            limits: self.limits,
            transport: self.transport.clone(),
            coordinator: self.coordinator.clone(),
        };
        let blob_transfer = self.blob_transfer.clone();
        let transport = self.transport.clone();
        tokio::spawn(async move {
            crate::resume::resume_incomplete_receives(blob_transfer, transport, &feature).await;
        });
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
