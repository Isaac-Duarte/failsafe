use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use failsafe_core::device::DeviceId;
use failsafe_transport::blobs::BlobTransfer;
use failsafe_transport::transport::Transport;
use tokio::time::MissedTickBehavior;
use tracing::{info, warn};

use crate::feature::SendFeature;
use crate::log::eprint_send;
use crate::transfer_state::{list_incomplete_receives, ReceiveStage, ReceiveTransferState};

pub async fn resume_incomplete_receives(
    blob_transfer: Arc<dyn BlobTransfer>,
    transport: Arc<dyn Transport>,
    feature: &SendFeature,
) {
    let states = match list_incomplete_receives().await {
        Ok(states) => states,
        Err(error) => {
            warn!("failed to list incomplete receives: {error}");
            return;
        }
    };

    if states.is_empty() {
        return;
    }

    let connected: Vec<DeviceId> = transport.connected_peers().await;
    for state in states {
        if !connected.contains(&state.sender) {
            continue;
        }
        let transfer_id = state.transfer_id;
        let sender = state.sender;
        info!(
            %transfer_id,
            sender = %sender,
            stage = ?state.stage,
            "resuming incomplete file receive"
        );
        match feature.resume_receive(blob_transfer.clone(), state).await {
            Ok(()) => {
                info!(%transfer_id, %sender, "resumed receive complete, sending acknowledgement");
                eprint_send(format_args!(
                    " resume complete for {transfer_id}, sending ack to {sender}"
                ));
                if let Err(error) = feature
                    .acknowledge_completed_receive(sender, transfer_id)
                    .await
                {
                    warn!(%transfer_id, %sender, %error, "failed to send receive acknowledgement");
                    eprint_send(format_args!(
                        " resume ack failed for {transfer_id}: {error}"
                    ));
                }
            }
            Err(error) => {
                warn!(%transfer_id, "failed to resume receive: {error}");
            }
        }
    }
}

/// Polls for newly connected peers and resumes any incomplete receives from them.
pub fn spawn_receive_resume_watcher(
    blob_transfer: Arc<dyn BlobTransfer>,
    transport: Arc<dyn Transport>,
    feature: Arc<SendFeature>,
) {
    tokio::spawn(async move {
        let mut known_peers = HashSet::new();
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            interval.tick().await;
            let connected = transport.connected_peers().await;
            let has_new_peer = connected.iter().any(|peer| !known_peers.contains(peer));
            known_peers = connected.into_iter().collect();
            if has_new_peer {
                resume_incomplete_receives(blob_transfer.clone(), transport.clone(), &feature).await;
            }
        }
    });
}

pub fn receive_state_from_payload(
    sender: DeviceId,
    payload: &crate::payload::SendPayload,
) -> ReceiveTransferState {
    let bytes_total = payload.entries.iter().map(|entry| entry.size).sum();
    ReceiveTransferState {
        transfer_id: payload.transfer_id,
        sender,
        sender_name: payload.sender_name.clone(),
        stage: ReceiveStage::Downloading,
        collection_hash: payload.collection_hash.clone(),
        entries: payload.entries.clone(),
        receive_dir: None,
        bytes_done: 0,
        bytes_total,
        error: None,
    }
}
