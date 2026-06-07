use std::sync::Arc;

use async_trait::async_trait;
use failsafe_clipboard::limits::ClipboardLimits;
use failsafe_core::control::{
    ControlError, ControlEvent, ControlResponse, ControlStream, SendPathSpec, SendPhase,
    send_response, write_event,
};
use failsafe_core::device::DeviceId;
use failsafe_core::feature::{ControlContext, FeatureControl, FeatureId, FeatureSpec};
use failsafe_core::message::FeatureMessage;
use failsafe_transport::blobs::BlobTransfer;
use failsafe_transport::transport::Transport;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::coordinator::SendCoordinator;
use crate::feature::SendFeatureSpec;
use crate::{
    SendProgress, SendProgressReporter, cancel_all_incomplete_receives, cancel_all_incomplete_sends,
    encode_envelope, eprint_send, mark_send_complete, mark_send_failed, plan_transfer_envelopes,
    prepare_send_payload, send_ack_timeout,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendFilesRequest {
    pub target: DeviceId,
    pub paths: Vec<SendPathSpec>,
    pub transfer_id: Uuid,
    #[serde(default)]
    pub resume: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SendControlBody {
    SendFiles(SendFilesRequest),
    CancelAll,
}

pub struct SendFeatureControl {
    transport: Arc<dyn Transport>,
    blob_transfer: Arc<dyn BlobTransfer>,
    device_name: String,
    send_limits: ClipboardLimits,
    coordinator: Arc<SendCoordinator>,
}

impl SendFeatureControl {
    pub fn new(
        transport: Arc<dyn Transport>,
        blob_transfer: Arc<dyn BlobTransfer>,
        device_name: String,
        send_limits: ClipboardLimits,
        coordinator: Arc<SendCoordinator>,
    ) -> Self {
        Self {
            transport,
            blob_transfer,
            device_name,
            send_limits,
            coordinator,
        }
    }

    async fn validate_preconditions(
        &self,
        ctx: &ControlContext<'_>,
        stream: &mut ControlStream,
        target: DeviceId,
    ) -> bool {
        let feature_id = SendFeatureSpec::feature_id();
        if !ctx.local_features.contains(&feature_id) {
            let _ = send_response(
                stream,
                &ControlResponse::Error {
                    message: "file_send is not enabled on this device; enable it in the web UI or with `failsafe devices features`, then wait for the daemon to sync".to_owned(),
                },
            )
            .await;
            return false;
        }

        if !ctx.peers.is_feature_enabled(target, feature_id).await {
            let _ = send_response(
                stream,
                &ControlResponse::Error {
                    message: format!(
                        "file_send is not enabled on device {target}; enable it on both devices"
                    ),
                },
            )
            .await;
            return false;
        }

        if !self.transport.connected_peers().await.contains(&target) {
            let _ = send_response(
                stream,
                &ControlResponse::Error {
                    message: format!("device {target} is offline or unreachable"),
                },
            )
            .await;
            return false;
        }

        true
    }

    async fn wait_for_send_ack(
        cancel: &CancellationToken,
        mut ack_rx: oneshot::Receiver<Result<(), String>>,
        receive_progress_rx: &mut mpsc::Receiver<SendProgress>,
        progress: &SendProgressReporter,
        ack_timeout: std::time::Duration,
        transfer_id: Uuid,
        target: DeviceId,
    ) -> Result<(), String> {
        info!(%transfer_id, %target, ?ack_timeout, "waiting for receiver acknowledgement");
        eprint_send(format_args!(
            " waiting for ack transfer_id={transfer_id} target={target}"
        ));
        let ack_deadline = tokio::time::sleep(ack_timeout);
        tokio::pin!(ack_deadline);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break Err("transfer cancelled".to_owned()),
                _ = &mut ack_deadline => {
                    warn!(%transfer_id, %target, ?ack_timeout, "timed out waiting for receiver acknowledgement");
                    eprint_send(format_args!(" ack timeout for {transfer_id}"));
                    break Err("timed out waiting for receiver acknowledgement".to_owned());
                }
                ack_result = &mut ack_rx => {
                    break match ack_result {
                        Ok(Ok(())) => {
                            info!(%transfer_id, %target, "receiver acknowledged file transfer");
                            eprint_send(format_args!(" ack received for {transfer_id}"));
                            Ok(())
                        }
                        Ok(Err(message)) => {
                            warn!(%transfer_id, %target, %message, "receiver reported transfer failure");
                            Err(message)
                        }
                        Err(_) => {
                            warn!(%transfer_id, %target, "acknowledgement channel closed before response");
                            Err("transfer acknowledgement channel closed".to_owned())
                        }
                    };
                }
                Some(receiver_progress) = receive_progress_rx.recv() => {
                    progress
                        .emit(
                            receiver_progress.phase,
                            receiver_progress.bytes_done,
                            receiver_progress.bytes_total,
                            receiver_progress.current_file,
                        )
                        .await;
                }
            }
        }
    }

    async fn handle_send_files(
        &self,
        ctx: &ControlContext<'_>,
        mut stream: ControlStream,
        request: SendFilesRequest,
    ) -> Result<(), ControlError> {
        if !self
            .validate_preconditions(ctx, &mut stream, request.target)
            .await
        {
            return Ok(());
        }

        if send_response(&mut stream, &ControlResponse::Ready)
            .await
            .is_err()
        {
            return Ok(());
        }

        let target = request.target;
        let transfer_id = request.transfer_id;
        let resume = request.resume;
        let paths = request.paths;

        let (mut read_half, mut write_half) = tokio::io::split(stream);

        let cancel = CancellationToken::new();
        let cancel_child = cancel.child_token();
        tokio::spawn(async move {
            let mut buf = [0u8; 64];
            loop {
                match read_half.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
            cancel_child.cancel();
        });

        let (progress_tx, mut progress_rx) = mpsc::channel::<ControlEvent>(1024);
        let progress_writer = tokio::spawn(async move {
            while let Some(event) = progress_rx.recv().await {
                if write_event(&mut write_half, &event).await.is_err() {
                    break;
                }
            }
            write_half
        });

        let (ack_rx, mut receive_progress_rx) = self.coordinator.register(transfer_id).await;
        info!(%transfer_id, %target, resume, "starting file send");
        eprint_send(format_args!(
            " starting send {transfer_id} -> {target} (resume={resume})"
        ));

        let progress = SendProgressReporter::new(progress_tx.clone());

        let mut result = async {
            let mut emit_progress: Box<dyn FnMut(SendPhase, u64, u64, Option<String>) + Send> =
                Box::new({
                    let progress = progress.clone();
                    move |phase, bytes_done, bytes_total, current_file| {
                        progress.try_emit(phase, bytes_done, bytes_total, current_file);
                    }
                });
            let payload = prepare_send_payload(
                &paths,
                target,
                self.blob_transfer.clone(),
                self.send_limits,
                self.device_name.clone(),
                transfer_id,
                resume,
                &cancel,
                &mut emit_progress,
            )
            .await?;

            if cancel.is_cancelled() {
                return Err("transfer cancelled".to_owned());
            }

            let total_bytes = payload.entries.iter().map(|entry| entry.size).sum::<u64>();

            progress
                .emit(SendPhase::Sending, total_bytes, total_bytes, None)
                .await;

            if cancel.is_cancelled() {
                return Err("transfer cancelled".to_owned());
            }

            let local_id = self.transport.local_device_id();
            let transfer_envelopes = plan_transfer_envelopes(local_id, target, payload);
            debug!(
                %transfer_id,
                messages = transfer_envelopes.len(),
                "sending transfer metadata"
            );
            for envelope in transfer_envelopes {
                let transfer_message = FeatureMessage::new(
                    local_id,
                    target,
                    SendFeatureSpec::feature_id(),
                    encode_envelope(&envelope),
                );
                self.transport
                    .send(transfer_message)
                    .await
                    .map_err(|error| error.to_string())?;
            }
            debug!(%transfer_id, "transfer metadata sent");

            progress
                .emit(SendPhase::WaitingForAck, 0, total_bytes, None)
                .await;

            let ack_timeout = send_ack_timeout(total_bytes);
            Self::wait_for_send_ack(
                &cancel,
                ack_rx,
                &mut receive_progress_rx,
                &progress,
                ack_timeout,
                transfer_id,
                target,
            )
            .await
        }
        .await;

        if cancel.is_cancelled() {
            self.coordinator.cancel(transfer_id).await;
            if !matches!(&result, Err(message) if message == "transfer cancelled") {
                result = Err("transfer cancelled".to_owned());
            }
        }

        progress.close();
        drop(progress);
        drop(progress_tx);

        let mut write_half = progress_writer
            .await
            .unwrap_or_else(|_| panic!("progress writer task panicked"));

        match result {
            Ok(()) => {
                let _ = mark_send_complete(transfer_id).await;
                let _ =
                    write_event(&mut write_half, &ControlEvent::SendComplete { transfer_id }).await;
            }
            Err(message) => {
                let _ = mark_send_failed(transfer_id, message.clone()).await;
                let _ = write_event(&mut write_half, &ControlEvent::SendFailed { message }).await;
            }
        }

        let _ = write_half.shutdown().await;
        Ok(())
    }

    async fn handle_cancel_all(&self, stream: &mut ControlStream) -> Result<(), ControlError> {
        let sends = match cancel_all_incomplete_sends(self.coordinator.as_ref()).await {
            Ok(count) => count,
            Err(message) => {
                send_response(stream, &ControlResponse::Error { message }).await?;
                return Ok(());
            }
        };
        let receives = match cancel_all_incomplete_receives().await {
            Ok(count) => count,
            Err(message) => {
                send_response(stream, &ControlResponse::Error { message }).await?;
                return Ok(());
            }
        };
        send_response(
            stream,
            &ControlResponse::CancelTransfers { sends, receives },
        )
        .await?;
        Ok(())
    }
}

#[async_trait]
impl FeatureControl for SendFeatureControl {
    fn control_id(&self) -> FeatureId {
        SendFeatureSpec::feature_id()
    }

    async fn handle_control(
        &self,
        ctx: &ControlContext<'_>,
        stream: ControlStream,
        body: serde_json::Value,
    ) -> Result<(), ControlError> {
        let body: SendControlBody = serde_json::from_value(body).map_err(|error| {
            ControlError::Config(format!("invalid file send control request: {error}"))
        })?;

        match body {
            SendControlBody::SendFiles(request) => {
                self.handle_send_files(ctx, stream, request).await?;
            }
            SendControlBody::CancelAll => {
                let mut stream = stream;
                self.handle_cancel_all(&mut stream).await?;
            }
        }

        Ok(())
    }
}
