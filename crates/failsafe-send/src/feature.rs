use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use failsafe_clipboard::limits::ClipboardLimits;
use failsafe_core::control::SendPhase;
use failsafe_core::device::DeviceId;
use failsafe_core::feature::{Feature, FeatureError, FeatureId, FeatureSpec};
use failsafe_core::message::FeatureMessage;
use failsafe_transport::blobs::{BlobHash, BlobTransfer};
use failsafe_transport::transport::{Transport, TransportError};
use tokio::sync::{Mutex, mpsc};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::coordinator::SendCoordinator;
use crate::inbound::receive_dir;
use crate::log::eprint_send;
use crate::manifest::ChunkedTransfer;
use crate::notify::notify_files_received;
use crate::payload::{
    SEND_PAYLOAD_VERSION, SendAck, SendEnvelope, SendProgress, decode_envelope, encode_envelope,
};
use crate::resume::receive_state_from_payload;
use crate::resume::spawn_receive_resume_watcher;
use crate::transfer_state::{
    ReceiveStage, ReceiveTransferState, load_receive_state, remove_receive_state,
    save_receive_state,
};

pub const ID: &str = "file_send";

pub struct SendFeatureSpec;

impl FeatureSpec for SendFeatureSpec {
    fn id() -> &'static str {
        ID
    }

    fn label() -> &'static str {
        "File Send"
    }

    fn description() -> &'static str {
        "Receive explicit file transfers from other devices"
    }
}

pub struct SendFeature {
    blob_transfer: Arc<dyn BlobTransfer>,
    limits: ClipboardLimits,
    transport: Arc<dyn Transport>,
    coordinator: Arc<SendCoordinator>,
    receive_in_progress: Arc<Mutex<HashMap<Uuid, Arc<Mutex<()>>>>>,
    pending_chunked_transfers: Arc<Mutex<HashMap<Uuid, ChunkedTransfer>>>,
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
            receive_in_progress: Arc::new(Mutex::new(HashMap::new())),
            pending_chunked_transfers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn clone_for_task(&self) -> Self {
        Self {
            blob_transfer: self.blob_transfer.clone(),
            limits: self.limits,
            transport: self.transport.clone(),
            coordinator: self.coordinator.clone(),
            receive_in_progress: self.receive_in_progress.clone(),
            pending_chunked_transfers: self.pending_chunked_transfers.clone(),
        }
    }

    pub async fn resume_receive(
        &self,
        blob_transfer: Arc<dyn BlobTransfer>,
        mut state: ReceiveTransferState,
    ) -> Result<(), String> {
        let transfer_id = state.transfer_id;
        let transfer_lock = {
            let mut locks = self.receive_in_progress.lock().await;
            locks
                .entry(transfer_id)
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _receive_guard = transfer_lock.lock().await;
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
        let transfer_id = payload.transfer_id;
        let transfer_lock = {
            let mut locks = self.receive_in_progress.lock().await;
            locks
                .entry(transfer_id)
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _receive_guard = transfer_lock.lock().await;
        let result = self.handle_transfer_locked(from, payload).await;
        drop(_receive_guard);
        let mut locks = self.receive_in_progress.lock().await;
        if locks.get(&transfer_id).is_some_and(|entry| {
            Arc::ptr_eq(entry, &transfer_lock) && Arc::strong_count(entry) == 2
        }) {
            locks.remove(&transfer_id);
        }
        result
    }

    async fn handle_transfer_locked(
        &self,
        from: DeviceId,
        payload: crate::payload::SendPayload,
    ) -> Result<(), FeatureError> {
        let transfer_id = payload.transfer_id;

        if payload.version != SEND_PAYLOAD_VERSION {
            return Err(FeatureError::Failed(
                SendFeatureSpec::feature_id(),
                format!(
                    "unsupported send payload version {} (expected {SEND_PAYLOAD_VERSION})",
                    payload.version
                ),
            ));
        }

        info!(
            %from,
            transfer_id = %payload.transfer_id,
            files = payload.entries.len(),
            bytes = payload.entries.iter().map(|entry| entry.size).sum::<u64>(),
            "received file transfer"
        );
        eprint_send(format_args!(
            " received transfer {} from {from} ({} files)",
            payload.transfer_id,
            payload.entries.len()
        ));

        let mut state = match load_receive_state(payload.transfer_id).await {
            Ok(saved) => {
                info!(
                    transfer_id = %payload.transfer_id,
                    stage = ?saved.stage,
                    "resuming saved receive state"
                );
                saved
            }
            Err(_) => receive_state_from_payload(from, &payload),
        };

        if state.stage == ReceiveStage::Complete {
            info!(
                transfer_id = %payload.transfer_id,
                "receive already complete, sending acknowledgement"
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
            self.remove_completed_receive_state(payload.transfer_id)
                .await;
            return Ok(());
        }

        state.sender = from;
        state.sender_name = payload.sender_name.clone();
        state.collection_hash = payload.collection_hash.clone();
        state.entries = payload.entries.clone();
        state.bytes_total = payload.entries.iter().map(|entry| entry.size).sum();

        match self
            .run_receive(self.blob_transfer.clone(), &mut state)
            .await
        {
            Ok(()) => {}
            Err(message) => {
                error!(
                    transfer_id = %payload.transfer_id,
                    %message,
                    "file receive failed before acknowledgement"
                );
                eprint_send(format_args!(
                    " receive failed for {}: {message}",
                    payload.transfer_id
                ));
                if let Ok(mut state) = load_receive_state(transfer_id).await {
                    state.stage = ReceiveStage::Failed;
                    state.error = Some(message.clone());
                    let _ = save_receive_state(&state).await;
                }
                let _ = self
                    .send_ack(
                        from,
                        SendAck {
                            transfer_id: payload.transfer_id,
                            ok: false,
                            error: Some(message.clone()),
                        },
                    )
                    .await;
                return Err(FeatureError::Failed(SendFeatureSpec::feature_id(), message));
            }
        }

        self.send_ack(
            from,
            SendAck {
                transfer_id: payload.transfer_id,
                ok: true,
                error: None,
            },
        )
        .await?;
        self.remove_completed_receive_state(payload.transfer_id)
            .await;

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
        info!(
            transfer_id = %state.transfer_id,
            bytes_total = state.bytes_total,
            "downloading file collection"
        );

        let root = BlobHash::from(state.collection_hash.as_str());
        let (progress_tx, mut progress_rx) = mpsc::channel::<SendProgress>(256);
        let progress_transport = self.transport.clone();
        let progress_sender = state.sender;
        let progress_task = tokio::spawn(async move {
            while let Some(progress) = progress_rx.recv().await {
                let transfer_id = progress.transfer_id;
                let local_id = progress_transport.local_device_id();
                let envelope = SendEnvelope::Progress(progress);
                let message = FeatureMessage::new(
                    local_id,
                    progress_sender,
                    SendFeatureSpec::feature_id(),
                    encode_envelope(&envelope),
                );
                if let Err(error) = progress_transport.send(message).await {
                    warn!(
                        %transfer_id,
                        to = %progress_sender,
                        %error,
                        "failed to send file receive progress"
                    );
                    break;
                }
            }
        });

        let transfer_id = state.transfer_id;
        let emit_progress =
            |phase: SendPhase, bytes_done: u64, bytes_total: u64, current_file: Option<String>| {
                let _ = progress_tx.try_send(SendProgress {
                    transfer_id,
                    phase,
                    bytes_done,
                    bytes_total,
                    current_file,
                });
            };
        emit_progress(
            SendPhase::WaitingForAck,
            state.bytes_done,
            state.bytes_total,
            None,
        );

        blob_transfer
            .download_collection(state.sender, &root, state.bytes_total, &mut |progress| {
                state.bytes_done = progress.bytes_done;
                emit_progress(
                    SendPhase::WaitingForAck,
                    progress.bytes_done,
                    progress.bytes_total,
                    progress.current_file,
                );
            })
            .await
            .map_err(|error| error.to_string())?;

        state.stage = ReceiveStage::Exporting;
        state.bytes_done = state.bytes_total;
        save_receive_state(state).await?;
        info!(transfer_id = %state.transfer_id, "exporting received files");

        let receive_dir = state
            .receive_dir
            .clone()
            .or_else(|| receive_dir(&state.sender_name, state.transfer_id))
            .ok_or_else(|| "downloads directory unavailable".to_owned())?;
        state.receive_dir = Some(receive_dir.clone());

        let paths = blob_transfer
            .export_collection(&root, &receive_dir, &mut |progress| {
                emit_progress(
                    SendPhase::Storing,
                    state.bytes_total,
                    state.bytes_total,
                    progress.current_file,
                );
            })
            .await
            .map_err(|error| error.to_string())?;
        emit_progress(
            SendPhase::Storing,
            state.bytes_total,
            state.bytes_total,
            None,
        );

        state.stage = ReceiveStage::Complete;
        state.bytes_done = state.bytes_total;
        save_receive_state(state).await?;

        notify_files_received(&state.sender_name, paths.len(), &receive_dir);

        info!(
            sender = %state.sender,
            transfer_id = %state.transfer_id,
            count = paths.len(),
            "received file send"
        );

        drop(progress_tx);
        let _ = progress_task.await;

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
        let transfer_id = ack.transfer_id;
        let ok = ack.ok;
        info!(%transfer_id, %to, ok, "sending file transfer acknowledgement");
        eprint_send(format_args!(
            " sending ack transfer_id={transfer_id} to={to} ok={ok}"
        ));

        let local_id = self.transport.local_device_id();
        let envelope = SendEnvelope::Ack(ack);
        let message = FeatureMessage::new(
            local_id,
            to,
            SendFeatureSpec::feature_id(),
            encode_envelope(&envelope),
        );
        match self.transport.send(message).await {
            Ok(()) => {
                info!(%transfer_id, %to, ok, "file transfer acknowledgement sent");
                eprint_send(format_args!(
                    " ack sent transfer_id={transfer_id} to={to} ok={ok}"
                ));
                Ok(())
            }
            Err(error) => {
                warn!(%transfer_id, %to, ok, %error, "failed to send file transfer acknowledgement");
                eprint_send(format_args!(
                    " ack send failed transfer_id={transfer_id} to={to}: {error}"
                ));
                Err(transport_error_to_feature_error(error))
            }
        }
    }

    async fn remove_completed_receive_state(&self, transfer_id: Uuid) {
        if let Err(error) = remove_receive_state(transfer_id).await {
            warn!(
                %transfer_id,
                %error,
                "failed to remove completed receive state"
            );
        }
    }

    fn spawn_transfer_handler(&self, from: DeviceId, payload: crate::payload::SendPayload) {
        let blob_transfer = self.blob_transfer.clone();
        let limits = self.limits;
        let transport = self.transport.clone();
        let coordinator = self.coordinator.clone();
        let receive_in_progress = self.receive_in_progress.clone();
        let pending_chunked_transfers = self.pending_chunked_transfers.clone();
        tokio::spawn(async move {
            let feature = SendFeature {
                blob_transfer,
                limits,
                transport,
                coordinator,
                receive_in_progress,
                pending_chunked_transfers,
            };
            if let Err(error) = feature.handle_transfer(from, payload).await {
                error!(%error, "file transfer handler failed");
            }
        });
    }

    async fn try_finalize_chunked_transfer(
        &self,
        from: DeviceId,
        transfer_id: Uuid,
    ) -> Result<bool, FeatureError> {
        let payload = {
            let mut pending = self.pending_chunked_transfers.lock().await;
            let Some(state) = pending.get(&transfer_id) else {
                return Ok(false);
            };
            if !state.is_complete() {
                return Ok(false);
            }
            let state = pending.remove(&transfer_id).expect("transfer state present");
            state
                .into_payload()
                .map_err(|error| FeatureError::Failed(SendFeatureSpec::feature_id(), error))?
        };
        self.spawn_transfer_handler(from, payload);
        Ok(true)
    }

    fn spawn_incomplete_chunk_timeout(&self, from: DeviceId, transfer_id: Uuid) {
        let pending_chunked_transfers = self.pending_chunked_transfers.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let state = pending_chunked_transfers
                .lock()
                .await
                .remove(&transfer_id);
            let Some(state) = state else {
                return;
            };
            if !state.is_complete() {
                let message = state.into_payload().unwrap_err();
                error!(
                    %transfer_id,
                    from = %from,
                    %message,
                    "timed out waiting for remaining transfer manifest chunks"
                );
                eprint_send(format_args!(
                    " manifest timeout for {transfer_id} from {from}: {message}"
                ));
            }
        });
    }
}

#[async_trait]
impl Feature for SendFeature {
    fn id(&self) -> FeatureId {
        SendFeatureSpec::feature_id()
    }

    async fn start(&mut self) -> Result<(), FeatureError> {
        let feature = Arc::new(self.clone_for_task());
        let blob_transfer = self.blob_transfer.clone();
        let transport = self.transport.clone();
        let feature_for_startup = feature.clone();
        tokio::spawn(async move {
            crate::resume::resume_incomplete_receives(
                blob_transfer,
                transport,
                &feature_for_startup,
            )
            .await;
        });
        spawn_receive_resume_watcher(self.blob_transfer.clone(), self.transport.clone(), feature);
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), FeatureError> {
        Ok(())
    }

    async fn handle_message(&mut self, message: FeatureMessage) -> Result<(), FeatureError> {
        let envelope = decode_envelope(&message.payload)?;
        match envelope {
            SendEnvelope::Transfer(payload) => {
                self.spawn_transfer_handler(message.from, payload);
                Ok(())
            }
            SendEnvelope::TransferHeader(header) => {
                let transfer_id = header.transfer_id;
                self.pending_chunked_transfers
                    .lock()
                    .await
                    .insert(transfer_id, ChunkedTransfer::new(header));
                Ok(())
            }
            SendEnvelope::TransferChunk(chunk) => {
                let transfer_id = chunk.transfer_id;
                {
                    let mut pending = self.pending_chunked_transfers.lock().await;
                    let Some(state) = pending.get_mut(&transfer_id) else {
                        return Err(FeatureError::Failed(
                            SendFeatureSpec::feature_id(),
                            format!(
                                "received transfer chunk for unknown transfer {transfer_id}"
                            ),
                        ));
                    };
                    state.push_chunk(chunk.chunk_index, chunk.entries);
                }
                self.try_finalize_chunked_transfer(message.from, transfer_id)
                    .await?;
                Ok(())
            }
            SendEnvelope::TransferEnd(end) => {
                let transfer_id = end.transfer_id;
                {
                    let mut pending = self.pending_chunked_transfers.lock().await;
                    let Some(state) = pending.get_mut(&transfer_id) else {
                        return Err(FeatureError::Failed(
                            SendFeatureSpec::feature_id(),
                            format!(
                                "received transfer end for unknown transfer {transfer_id}"
                            ),
                        ));
                    };
                    state.mark_end(end.chunk_count);
                }
                if self
                    .try_finalize_chunked_transfer(message.from, transfer_id)
                    .await?
                {
                    return Ok(());
                }
                // End can arrive on a faster stream before the last chunks; wait for them.
                self.spawn_incomplete_chunk_timeout(message.from, transfer_id);
                Ok(())
            }
            SendEnvelope::Ack(ack) => self.handle_ack(ack).await,
            SendEnvelope::Progress(progress) => {
                self.coordinator.report_progress(progress).await;
                Ok(())
            }
        }
    }
}

fn transport_error_to_feature_error(error: TransportError) -> FeatureError {
    FeatureError::Failed(SendFeatureSpec::feature_id(), error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use failsafe_core::feature::{Feature, FeatureSpec};
    use failsafe_core::message::FeatureMessage;
    use failsafe_transport::blobs::MockBlobTransfer;
    use failsafe_transport::mock::MockTransport;
    use uuid::Uuid;

    use super::*;
    use crate::payload::{FileEntry, SEND_PAYLOAD_VERSION, SendPayload};

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
        let transfer_id = payload.transfer_id;

        feature
            .handle_message(FeatureMessage::new(
                local_id,
                peer_id,
                SendFeatureSpec::feature_id(),
                encode_envelope(&SendEnvelope::Transfer(payload)),
            ))
            .await
            .unwrap();

        let mut saw_progress = false;
        for _ in 0..50 {
            if let Ok(Some(ack_message)) = local_transport.try_recv().await {
                assert_eq!(ack_message.feature, SendFeatureSpec::feature_id());
                match decode_envelope(&ack_message.payload).unwrap() {
                    SendEnvelope::Progress(progress) => {
                        assert_eq!(progress.transfer_id, transfer_id);
                        assert!(progress.bytes_done <= progress.bytes_total);
                        saw_progress = true;
                    }
                    SendEnvelope::Ack(SendAck { ok: true, .. }) => {
                        assert!(saw_progress);
                        return;
                    }
                    other => panic!("unexpected envelope: {other:?}"),
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("timed out waiting for transfer acknowledgement");
    }
}
