use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, oneshot};
use uuid::Uuid;

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
        self.pending.lock().await.insert(transfer_id, tx);
        rx
    }

    pub async fn complete(&self, transfer_id: Uuid, result: Result<(), String>) {
        if let Some(tx) = self.pending.lock().await.remove(&transfer_id) {
            let _ = tx.send(result);
        }
    }

    pub async fn cancel(&self, transfer_id: Uuid) {
        self.complete(transfer_id, Err("transfer cancelled".to_owned()))
            .await;
    }
}

impl Default for SendCoordinator {
    fn default() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }
}
