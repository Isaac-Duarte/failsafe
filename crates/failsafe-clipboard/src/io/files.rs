use std::path::{Path, PathBuf};

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
    tokio::fs::create_dir_all(&session)
        .await
        .map_err(|error| ClipboardIoError::Unavailable(error.to_string()))?;

    let mut paths = Vec::with_capacity(files.len());
    for (name, data) in files {
        let path = session.join(sanitize_filename(name));
        tokio::fs::write(&path, data)
            .await
            .map_err(|error| ClipboardIoError::Unavailable(error.to_string()))?;
        paths.push(path);
    }

    Ok(paths)
}

fn sanitize_filename(name: &str) -> String {
    let candidate = Path::new(name)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    if candidate.is_empty() {
        "file".to_owned()
    } else {
        candidate.to_owned()
    }
}
