use std::path::PathBuf;
use std::sync::Arc;

use failsafe_clipboard::limits::ClipboardLimits;
use failsafe_core::control::{SendPathSpec, SendPhase};
use failsafe_core::device::DeviceId;
use failsafe_transport::blobs::{BlobHash, BlobProgress, BlobTransfer, ImportedFile};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::files::{collect_file_preview, collect_import_sources};
use crate::payload::{FileEntry, SEND_PAYLOAD_VERSION, SendPayload};
use crate::transfer_state::{SendStage, SendTransferState, save_send_state};

type ProgressFn<'a> = &'a mut Box<dyn FnMut(SendPhase, u64, u64, Option<String>) + Send>;

pub async fn prepare_send_payload(
    paths: &[SendPathSpec],
    target: DeviceId,
    blob_transfer: Arc<dyn BlobTransfer>,
    limits: ClipboardLimits,
    sender_name: String,
    transfer_id: Uuid,
    resume: bool,
    cancel: &CancellationToken,
    progress: ProgressFn<'_>,
) -> Result<SendPayload, String> {
    if cancel.is_cancelled() {
        return Err("transfer cancelled".to_owned());
    }

    let (send_paths, mut state) =
        load_or_init_send_state(paths, target, transfer_id, resume).await?;

    limits.validate_entries(
        &state
            .entries
            .iter()
            .map(|entry| (entry.name.clone(), entry.size))
            .collect::<Vec<_>>(),
    )?;

    save_send_state(&state).await?;

    let total_bytes = state.bytes_total;
    let mut collection_hash = state.collection_hash.clone();
    let mut entries = state.entries.clone();

    let import_needed = collection_import_needed(
        blob_transfer.as_ref(),
        collection_hash.as_deref(),
        total_bytes,
    )
    .await;

    if import_needed {
        state.stage = SendStage::Importing;
        save_send_state(&state).await?;

        progress(SendPhase::Preparing, 0, total_bytes, None);
        let sources = collect_import_sources(&send_paths)?;
        let (hash, imported) = run_collection_import(
            blob_transfer,
            &sources,
            cancel,
            progress,
        )
        .await?;

        collection_hash = Some(hash.as_str().to_owned());
        entries = imported
            .into_iter()
            .map(|file| FileEntry {
                name: file.name,
                size: file.size,
            })
            .collect();
        state.collection_hash = collection_hash.clone();
        state.entries = entries.clone();
        state.bytes_done = total_bytes;
        state.stage = SendStage::ReadyToSend;
        save_send_state(&state).await?;
        progress(SendPhase::Storing, total_bytes, total_bytes, None);
    } else {
        emit_prepared_progress(progress, total_bytes);
    }

    if cancel.is_cancelled() {
        return Err("transfer cancelled".to_owned());
    }

    state.stage = SendStage::WaitingAck;
    save_send_state(&state).await?;

    Ok(SendPayload {
        version: SEND_PAYLOAD_VERSION,
        transfer_id,
        sender_name,
        collection_hash: collection_hash
            .ok_or_else(|| "collection hash missing after import".to_owned())?,
        entries,
    })
}

async fn load_or_init_send_state(
    paths: &[SendPathSpec],
    target: DeviceId,
    transfer_id: Uuid,
    resume: bool,
) -> Result<(Vec<SendPathSpec>, SendTransferState), String> {
    if resume {
        let saved = crate::transfer_state::load_send_state(transfer_id).await?;
        if saved.stage == SendStage::Complete {
            return Err("transfer already completed".to_owned());
        }
        Ok((saved.paths.clone(), saved))
    } else {
        let previews = collect_file_preview(paths)?;
        let bytes_total: u64 = previews.iter().map(|preview| preview.size).sum();
        Ok((
            paths.to_vec(),
            SendTransferState {
                transfer_id,
                target,
                paths: paths.to_vec(),
                stage: SendStage::Importing,
                collection_hash: None,
                entries: previews
                    .into_iter()
                    .map(|preview| FileEntry {
                        name: preview.name,
                        size: preview.size,
                    })
                    .collect(),
                bytes_done: 0,
                bytes_total,
                error: None,
            },
        ))
    }
}

async fn collection_import_needed(
    blob_transfer: &dyn BlobTransfer,
    collection_hash: Option<&str>,
    total_bytes: u64,
) -> bool {
    match collection_hash {
        Some(hash) => blob_transfer
            .collection_status(&BlobHash::from(hash), total_bytes)
            .await
            .map(|(_, _, complete)| !complete)
            .unwrap_or(true),
        None => true,
    }
}

async fn run_collection_import(
    blob_transfer: Arc<dyn BlobTransfer>,
    sources: &[(String, PathBuf)],
    cancel: &CancellationToken,
    progress: ProgressFn<'_>,
) -> Result<(BlobHash, Vec<ImportedFile>), String> {
    let mut on_import_progress = |blob_progress: BlobProgress| {
        progress(
            SendPhase::Preparing,
            blob_progress.bytes_done,
            blob_progress.bytes_total,
            blob_progress.current_file,
        );
    };
    let mut import_future =
        Box::pin(blob_transfer.import_sources(sources, &mut on_import_progress));
    tokio::select! {
        _ = cancel.cancelled() => Err("transfer cancelled".to_owned()),
        result = import_future.as_mut() => result.map_err(|error| error.to_string()),
    }
}

fn emit_prepared_progress(progress: ProgressFn<'_>, total_bytes: u64) {
    progress(SendPhase::Preparing, total_bytes, total_bytes, None);
    progress(SendPhase::Storing, total_bytes, total_bytes, None);
}

pub async fn mark_send_complete(transfer_id: Uuid) -> Result<(), String> {
    let mut state = crate::transfer_state::load_send_state(transfer_id).await?;
    state.stage = SendStage::Complete;
    state.error = None;
    save_send_state(&state).await?;
    crate::transfer_state::remove_send_state(transfer_id).await
}

pub async fn mark_send_failed(transfer_id: Uuid, message: String) -> Result<(), String> {
    if let Ok(mut state) = crate::transfer_state::load_send_state(transfer_id).await {
        state.stage = SendStage::Failed;
        state.error = Some(message);
        let _ = save_send_state(&state).await;
    }
    Ok(())
}
