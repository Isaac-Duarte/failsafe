use std::path::PathBuf;

use failsafe_core::path::write_named_files;

use super::ClipboardIoError;

pub fn default_clipboard_cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("failsafe").join("clipboard"))
}

pub async fn write_received_files(
    files: &[(String, Vec<u8>)],
) -> Result<Vec<PathBuf>, ClipboardIoError> {
    let base = default_clipboard_cache_dir().ok_or_else(|| {
        ClipboardIoError::Unavailable("clipboard cache dir unavailable".to_owned())
    })?;
    let session = base.join(uuid::Uuid::new_v4().to_string());

    write_named_files(&session, files)
        .await
        .map_err(|error| ClipboardIoError::Unavailable(error.to_string()))
}
