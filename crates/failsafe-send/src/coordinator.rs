use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, oneshot};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::log::eprint_send;
use crate::payload::SendAck;

pub struct SendCoordinator {
    pending: Mutex<HashMap<Uuid, oneshot::Sender<Result<(), String>>>>,
}

impl SendCoordinator {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            pending: Mutex::new(HashMap::new()),
        })
    }

    pub async fn register(&self, transfer_id: Uuid) -> oneshot::Receiver<Result<(), String>> {
        let (tx, rx) = oneshot::channel();
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
        pending.insert(transfer_id, tx);
        info!(%transfer_id, pending = pending.len(), "registered send acknowledgement waiter");
        eprint_send(format_args!(" registered ack waiter for {transfer_id}"));
        rx
    }

    pub async fn complete(&self, transfer_id: Uuid, result: Result<(), String>) {
        let tx = self.pending.lock().await.remove(&transfer_id);
        match tx {
            Some(tx) => {
                let ok = result.is_ok();
                info!(%transfer_id, ok, "completing send acknowledgement waiter");
                eprint_send(format_args!(
                    " completed ack waiter for {transfer_id} (ok={ok})"
                ));
                let _ = tx.send(result);
            }
            None => {
                let pending: Vec<Uuid> = self.pending.lock().await.keys().copied().collect();
                warn!(
                    %transfer_id,
                    ?pending,
                    ok = result.is_ok(),
                    "received acknowledgement for transfer with no registered waiter"
                );
                eprint_send(format_args!(
                    " orphan ack for {transfer_id} (ok={}); pending: {pending:?}",
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
}

impl Default for SendCoordinator {
    fn default() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }
}
