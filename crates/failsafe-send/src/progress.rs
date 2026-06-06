use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use failsafe_core::control::{ControlEvent, SendPhase};
use tokio::sync::{mpsc, Notify};

#[derive(Clone)]
struct ProgressSnapshot {
    phase: SendPhase,
    bytes_done: u64,
    bytes_total: u64,
    current_file: Option<String>,
}

#[derive(Clone)]
pub struct SendProgressReporter {
    tx: mpsc::Sender<ControlEvent>,
    sequence: Arc<AtomicU64>,
    pending: Arc<Mutex<Option<ProgressSnapshot>>>,
    notify: Arc<Notify>,
}

impl SendProgressReporter {
    pub fn new(tx: mpsc::Sender<ControlEvent>) -> Self {
        let sequence = Arc::new(AtomicU64::new(0));
        let pending = Arc::new(Mutex::new(None));
        let notify = Arc::new(Notify::new());

        let flush_tx = tx.clone();
        let flush_sequence = sequence.clone();
        let flush_pending = pending.clone();
        let flush_notify = notify.clone();

        tokio::spawn(async move {
            loop {
                flush_notify.notified().await;
                tokio::time::sleep(Duration::from_millis(100)).await;

                let snapshot: Option<ProgressSnapshot> = flush_pending.lock().unwrap().take();
                let Some(snapshot) = snapshot else {
                    continue;
                };

                let sequence = flush_sequence.fetch_add(1, Ordering::Relaxed) + 1;
                let _ = flush_tx
                    .send(ControlEvent::SendProgress {
                        sequence,
                        phase: snapshot.phase,
                        bytes_done: snapshot.bytes_done,
                        bytes_total: snapshot.bytes_total,
                        current_file: snapshot.current_file,
                    })
                    .await;
            }
        });

        Self {
            tx,
            sequence,
            pending,
            notify,
        }
    }

    pub fn try_emit(
        &self,
        phase: SendPhase,
        bytes_done: u64,
        bytes_total: u64,
        current_file: Option<String>,
    ) {
        *self.pending.lock().unwrap() = Some(ProgressSnapshot {
            phase,
            bytes_done,
            bytes_total,
            current_file,
        });
        self.notify.notify_one();
    }

    pub async fn emit(
        &self,
        phase: SendPhase,
        bytes_done: u64,
        bytes_total: u64,
        current_file: Option<String>,
    ) {
        *self.pending.lock().unwrap() = None;
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
