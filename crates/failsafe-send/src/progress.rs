use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use failsafe_core::control::{ControlEvent, SendPhase};
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct SendProgressReporter {
    tx: mpsc::Sender<ControlEvent>,
    sequence: Arc<AtomicU64>,
}

impl SendProgressReporter {
    pub fn new(tx: mpsc::Sender<ControlEvent>) -> Self {
        Self {
            tx,
            sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn try_emit(
        &self,
        phase: SendPhase,
        bytes_done: u64,
        bytes_total: u64,
        current_file: Option<String>,
    ) {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self.tx.try_send(ControlEvent::SendProgress {
            sequence,
            phase,
            bytes_done,
            bytes_total,
            current_file,
        });
    }

    pub async fn emit(
        &self,
        phase: SendPhase,
        bytes_done: u64,
        bytes_total: u64,
        current_file: Option<String>,
    ) {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self
            .tx
            .send(ControlEvent::SendProgress {
                sequence,
                phase,
                bytes_done,
                bytes_total,
                current_file,
            })
            .await;
    }
}
