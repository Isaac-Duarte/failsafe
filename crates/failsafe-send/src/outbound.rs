use std::path::PathBuf;
use std::sync::Arc;

use failsafe_clipboard::limits::ClipboardLimits;
use failsafe_core::control::SendPhase;
use failsafe_transport::blobs::BlobTransfer;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::files::read_files_from_paths;
use crate::payload::{FileEntry, SendPayload, SEND_PAYLOAD_VERSION};

pub async fn prepare_send_payload(
    paths: &[PathBuf],
    blob_transfer: Arc<dyn BlobTransfer>,
    limits: ClipboardLimits,
    sender_name: String,
    transfer_id: Uuid,
    cancel: &CancellationToken,
    mut progress: impl FnMut(SendPhase, u64, u64, Option<String>),
) -> Result<SendPayload, String> {
    if cancel.is_cancelled() {
        return Err("transfer cancelled".to_owned());
    }

    progress(SendPhase::Preparing, 0, 0, None);

    let files = read_files_from_paths(paths, limits, |bytes_done, bytes_total, current| {
        progress(
            SendPhase::Preparing,
            bytes_done,
            bytes_total,
            Some(current.to_owned()),
        );
    })
    .await?;

    if cancel.is_cancelled() {
        return Err("transfer cancelled".to_owned());
    }

    let total_bytes: u64 = files.iter().map(|(_, data)| data.len() as u64).sum();
    progress(SendPhase::Storing, 0, total_bytes, None);

    let hash = blob_transfer
        .store_files(files.clone())
        .await
        .map_err(|error| error.to_string())?;

    progress(SendPhase::Storing, total_bytes, total_bytes, None);

    let entries = files
        .into_iter()
        .map(|(name, data)| FileEntry {
            name,
            size: data.len() as u64,
        })
        .collect();

    Ok(SendPayload {
        version: SEND_PAYLOAD_VERSION,
        transfer_id,
        sender_name,
        collection_hash: hash.as_str().to_owned(),
        entries,
    })
}

