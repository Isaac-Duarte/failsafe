use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, mpsc, oneshot};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::log::eprint_send;
use crate::payload::{SendAck, SendProgress};

struct PendingSend {
    ack: oneshot::Sender<Result<(), String>>,
    progress: mpsc::Sender<SendProgress>,
}

pub struct SendCoordinator {
    pending: Mutex<HashMap<Uuid, PendingSend>>,
}

impl SendCoordinator {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            pending: Mutex::new(HashMap::new()),
        })
    }

    pub async fn register(
        &self,
        transfer_id: Uuid,
    ) -> (
        oneshot::Receiver<Result<(), String>>,
        mpsc::Receiver<SendProgress>,
    ) {
        let (ack_tx, ack_rx) = oneshot::channel();
        let (progress_tx, progress_rx) = mpsc::channel(256);
        let mut pending = self.pending.lock().await;
        if pending.contains_key(&transfer_id) {
            warn!(
                %transfer_id,
                "replacing existing acknowledgement waiter for transfer"
            );
            eprint_send(format_args!(
                " warning: replacing existing ack waiter for {transfer_id}"
            ));
        }
        pending.insert(
            transfer_id,
            PendingSend {
                ack: ack_tx,
                progress: progress_tx,
            },
        );
        info!(%transfer_id, pending = pending.len(), "registered send acknowledgement waiter");
        eprint_send(format_args!(" registered ack waiter for {transfer_id}"));
        (ack_rx, progress_rx)
    }

    pub async fn complete(&self, transfer_id: Uuid, result: Result<(), String>) {
        let mut pending = self.pending.lock().await;
        let pending_send = pending.remove(&transfer_id);
        match pending_send {
            Some(pending_send) => {
                let ok = result.is_ok();
                info!(%transfer_id, ok, "completing send acknowledgement waiter");
                eprint_send(format_args!(
                    " completed ack waiter for {transfer_id} (ok={ok})"
                ));
                let _ = pending_send.ack.send(result);
            }
            None => {
                let pending_ids: Vec<Uuid> = pending.keys().copied().collect();
                warn!(
                    %transfer_id,
                    ?pending_ids,
                    ok = result.is_ok(),
                    "received acknowledgement for transfer with no registered waiter"
                );
                eprint_send(format_args!(
                    " orphan ack for {transfer_id} (ok={}); pending: {pending_ids:?}",
                    result.is_ok()
                ));
            }
        }
    }

    pub async fn cancel(&self, transfer_id: Uuid) {
        debug!(%transfer_id, "cancelling send acknowledgement waiter");
        self.complete(transfer_id, Err("transfer cancelled".to_owned()))
            .await;
    }

    pub async fn cancel_all(&self) {
        let pending: Vec<Uuid> = self.pending.lock().await.keys().copied().collect();
        for transfer_id in pending {
            self.cancel(transfer_id).await;
        }
    }

    pub async fn complete_ack(&self, ack: SendAck) {
        info!(
            transfer_id = %ack.transfer_id,
            ok = ack.ok,
            error = ack.error.as_deref().unwrap_or(""),
            "received send acknowledgement"
        );
        eprint_send(format_args!(
            " inbound ack transfer_id={} ok={}",
            ack.transfer_id, ack.ok
        ));
        let result = if ack.ok {
            Ok(())
        } else {
            Err(ack
                .error
                .unwrap_or_else(|| "receiver reported failure".to_owned()))
        };
        self.complete(ack.transfer_id, result).await;
    }

    pub async fn report_progress(&self, progress: SendProgress) {
        let tx = self
            .pending
            .lock()
            .await
            .get(&progress.transfer_id)
            .map(|pending| pending.progress.clone());
        let Some(tx) = tx else {
            debug!(
                transfer_id = %progress.transfer_id,
                "received progress for transfer with no registered waiter"
            );
            return;
        };
        let _ = tx.send(progress).await;
    }
}

impl Default for SendCoordinator {
    fn default() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use failsafe_core::control::SendPhase;

    use super::*;

    #[tokio::test]
    async fn registered_send_receives_progress_and_ack() {
        let coordinator = SendCoordinator::new();
        let transfer_id = Uuid::new_v4();
        let (ack_rx, mut progress_rx) = coordinator.register(transfer_id).await;

        coordinator
            .report_progress(SendProgress {
                transfer_id,
                phase: SendPhase::WaitingForAck,
                bytes_done: 5,
                bytes_total: 10,
                current_file: None,
            })
            .await;
        coordinator.complete(transfer_id, Ok(())).await;

        let progress = progress_rx.recv().await.expect("progress update");
        assert_eq!(progress.bytes_done, 5);
        assert_eq!(progress.bytes_total, 10);
        assert!(ack_rx.await.expect("ack channel").is_ok());
    }
}
