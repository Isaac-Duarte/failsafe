use std::path::{Path, PathBuf};

use failsafe_clipboard::limits::ClipboardLimits;
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePreview {
    pub name: String,
    pub size: u64,
}

pub fn collect_import_sources(paths: &[PathBuf]) -> Result<Vec<(String, PathBuf)>, String> {
    let mut sources = Vec::new();
    for path in paths {
        collect_path_sources(path, &mut sources)?;
    }
    if sources.is_empty() {
        return Err("no files to send".to_owned());
    }
    sources.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(sources)
}

fn collect_path_sources(path: &Path, sources: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("failed to resolve {}: {error}", path.display()))?;
    if !canonical.exists() {
        return Err(format!("path does not exist: {}", path.display()));
    }

    let metadata = std::fs::metadata(&canonical)
        .map_err(|error| format!("failed to stat {}: {error}", canonical.display()))?;

    if metadata.is_file() {
        let name = canonical
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("file")
            .to_owned();
        sources.push((name, canonical));
        return Ok(());
    }

    if metadata.is_dir() {
        let root = canonical
            .parent()
            .ok_or_else(|| format!("directory has no parent: {}", canonical.display()))?;
        for entry in WalkDir::new(&canonical).into_iter() {
            let entry = entry
                .map_err(|error| format!("failed to walk {}: {error}", canonical.display()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let file_path = entry.into_path();
            let relative = file_path
                .strip_prefix(root)
                .map_err(|error| format!("failed to relativize path: {error}"))?;
            let name = relative
                .to_str()
                .ok_or_else(|| format!("non-utf8 path: {}", relative.display()))?
                .trim_start_matches('/')
                .to_owned();
            sources.push((name, file_path));
        }
    }

    Ok(())
}

pub fn collect_file_preview(paths: &[PathBuf]) -> Result<Vec<FilePreview>, String> {
    collect_import_sources(paths).map(|sources| {
        sources
            .into_iter()
            .map(|(name, path)| FilePreview {
                size: std::fs::metadata(&path)
                    .map(|metadata| metadata.len())
                    .unwrap_or_default(),
                name,
            })
            .collect()
    })
}

pub async fn read_files_from_paths(
    paths: &[PathBuf],
    limits: ClipboardLimits,
    mut on_progress: impl FnMut(u64, u64, &str),
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let previews = collect_file_preview(paths)?;
    let total_bytes: u64 = previews.iter().map(|preview| preview.size).sum();
    let mut files = Vec::with_capacity(previews.len());
    let mut bytes_done = 0u64;

    for path in paths {
        read_path_files(
            path,
            &mut files,
            limits,
            total_bytes,
            &mut bytes_done,
            &mut on_progress,
        )
        .await?;
    }

    if files.is_empty() {
        return Err("no readable files found".to_owned());
    }

    limits.validate_files(&files)?;
    Ok(files)
}

async fn read_path_files(
    path: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
    limits: ClipboardLimits,
    total_bytes: u64,
    bytes_done: &mut u64,
    on_progress: &mut impl FnMut(u64, u64, &str),
) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("path does not exist: {}", path.display()));
    }

    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|error| format!("failed to stat {}: {error}", path.display()))?;

    if metadata.is_file() {
        read_single_file(path, files, limits, total_bytes, bytes_done, on_progress).await?;
        return Ok(());
    }

    if metadata.is_dir() {
        let mut entries = tokio::fs::read_dir(path)
            .await
            .map_err(|error| format!("failed to read dir {}: {error}", path.display()))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|error| format!("failed to read dir entry in {}: {error}", path.display()))?
        {
            let entry_path = entry.path();
            if entry_path.is_file() {
                read_single_file(
                    &entry_path,
                    files,
                    limits,
                    total_bytes,
                    bytes_done,
                    on_progress,
                )
                .await?;
            }
        }
    }

    Ok(())
}

async fn read_single_file(
    path: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
    limits: ClipboardLimits,
    total_bytes: u64,
    bytes_done: &mut u64,
    on_progress: &mut impl FnMut(u64, u64, &str),
) -> Result<(), String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("file")
        .to_owned();
    on_progress(*bytes_done, total_bytes, &name);

    let data = tokio::fs::read(path)
        .await
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    limits.validate_blob(data.len())?;
    *bytes_done = bytes_done.saturating_add(data.len() as u64);
    on_progress(*bytes_done, total_bytes, &name);
    files.push((name, data));
    Ok(())
}

pub fn format_bytes(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size >= GB {
        format!("{:.1} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.1} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.1} KB", size as f64 / KB as f64)
    } else {
        format!("{size} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn preview_lists_files_in_directory() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("a.txt"), b"hello").unwrap();
        std::fs::write(temp.path().join("b.txt"), b"world!").unwrap();

        let mut previews = collect_file_preview(&[temp.path().to_path_buf()]).unwrap();
        assert_eq!(previews.len(), 2);
        previews.sort_by(|left, right| left.name.cmp(&right.name));
        assert_eq!(previews[0].size, 5);
        assert_eq!(previews[1].size, 6);
    }

    #[tokio::test]
    async fn rejects_oversized_file() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("big.bin");
        std::fs::write(&path, vec![0u8; 1024]).unwrap();

        let limits = ClipboardLimits {
            max_file_bytes: 512,
            max_total_bytes: 1024,
        };

        let result = read_files_from_paths(&[path], limits, |_, _, _| {}).await;
        assert!(result.is_err());
    }
}
