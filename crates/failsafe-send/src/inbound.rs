use std::path::{Path, PathBuf};

use chrono::Local;
use uuid::Uuid;

pub fn downloads_base_dir() -> Option<PathBuf> {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .map(|dir| dir.join("failsafe"))
}

pub fn receive_dir(sender_name: &str, transfer_id: Uuid) -> Option<PathBuf> {
    let base = downloads_base_dir()?;
    let safe_sender = sanitize_path_component(sender_name);
    let timestamp = Local::now().format("%Y%m%d-%H%M%S");
    let short_id = &transfer_id.to_string()[..8];
    Some(
        base.join(safe_sender)
            .join(format!("{timestamp}-{short_id}")),
    )
}

pub async fn save_received_files(
    sender_name: &str,
    transfer_id: Uuid,
    files: &[(String, Vec<u8>)],
) -> Result<Vec<PathBuf>, String> {
    let dir = receive_dir(sender_name, transfer_id)
        .ok_or_else(|| "downloads directory unavailable".to_owned())?;
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|error| format!("failed to create receive dir: {error}"))?;

    let mut paths = Vec::with_capacity(files.len());
    for (name, data) in files {
        let path = dir.join(sanitize_filename(name));
        tokio::fs::write(&path, data)
            .await
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        paths.push(path);
    }

    Ok(paths)
}

fn sanitize_path_component(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "device".to_owned();
    }
    trimmed
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn writes_files_to_receive_dir() {
        let sender = "test-device";
        let transfer_id = Uuid::new_v4();
        let dir = receive_dir(sender, transfer_id).expect("downloads dir");
        let _ = tokio::fs::remove_dir_all(&dir).await;

        let paths = save_received_files(
            sender,
            transfer_id,
            &[("hello.txt".to_owned(), b"hello".to_vec())],
        )
        .await
        .unwrap();

        assert_eq!(paths.len(), 1);
        assert!(paths[0].exists());
        let contents = tokio::fs::read(&paths[0]).await.unwrap();
        assert_eq!(contents, b"hello");

        let _ = tokio::fs::remove_dir_all(dir.parent().unwrap()).await;
    }
}
