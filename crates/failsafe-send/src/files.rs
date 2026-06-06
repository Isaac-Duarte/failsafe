use std::path::{Component, Path, PathBuf};

use failsafe_clipboard::limits::ClipboardLimits;
use failsafe_core::control::SendPathSpec;
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePreview {
    pub name: String,
    pub size: u64,
}

pub fn prepare_send_paths(paths: &[PathBuf]) -> Result<Vec<SendPathSpec>, String> {
    paths
        .iter()
        .map(|path| {
            Ok(SendPathSpec {
                label: normalize_path_label(path),
                local: resolve_send_path(path)?,
            })
        })
        .collect()
}

pub fn normalize_path_label(path: &Path) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir => {
                parts.pop();
            }
            Component::RootDir | Component::Prefix(_) => {}
        }
    }
    parts.join("/")
}

pub fn resolve_send_path(path: &Path) -> Result<PathBuf, String> {
    let resolved = if path.is_relative() {
        std::env::current_dir()
            .map_err(|error| format!("failed to get current directory: {error}"))?
            .join(path)
    } else {
        path.to_path_buf()
    };
    resolved
        .canonicalize()
        .map_err(|error| format!("failed to resolve {}: {error}", path.display()))
}

pub fn collect_import_sources(paths: &[SendPathSpec]) -> Result<Vec<(String, PathBuf)>, String> {
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

fn collect_path_sources(
    path: &SendPathSpec,
    sources: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    if path.label.is_empty() {
        return collect_path_sources_legacy(&path.local, sources);
    }

    let local = &path.local;
    if !local.exists() {
        return Err(format!("path does not exist: {}", local.display()));
    }

    let metadata = std::fs::metadata(local)
        .map_err(|error| format!("failed to stat {}: {error}", local.display()))?;

    if metadata.is_file() {
        sources.push((path.label.clone(), local.clone()));
        return Ok(());
    }

    if metadata.is_dir() {
        for entry in WalkDir::new(local).into_iter() {
            let entry = entry
                .map_err(|error| format!("failed to walk {}: {error}", local.display()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let file_path = entry.into_path();
            let relative = file_path
                .strip_prefix(local)
                .map_err(|error| format!("failed to relativize path: {error}"))?;
            let name = join_archive_path(&path.label, relative)?;
            sources.push((name, file_path));
        }
    }

    Ok(())
}

fn collect_path_sources_legacy(
    local: &Path,
    sources: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    if !local.exists() {
        return Err(format!("path does not exist: {}", local.display()));
    }

    let metadata = std::fs::metadata(local)
        .map_err(|error| format!("failed to stat {}: {error}", local.display()))?;

    if metadata.is_file() {
        let name = local
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("file")
            .to_owned();
        sources.push((name, local.to_path_buf()));
        return Ok(());
    }

    if metadata.is_dir() {
        let root = local
            .parent()
            .ok_or_else(|| format!("directory has no parent: {}", local.display()))?;
        for entry in WalkDir::new(local).into_iter() {
            let entry = entry
                .map_err(|error| format!("failed to walk {}: {error}", local.display()))?;
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

fn join_archive_path(prefix: &str, relative: &Path) -> Result<String, String> {
    let relative = relative
        .to_str()
        .ok_or_else(|| format!("non-utf8 path: {}", relative.display()))?
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_owned();
    if relative.is_empty() {
        return Err("encountered empty relative path while walking directory".to_owned());
    }
    if prefix.is_empty() {
        return Ok(relative);
    }
    Ok(format!("{prefix}/{relative}"))
}

pub fn collect_file_preview(paths: &[SendPathSpec]) -> Result<Vec<FilePreview>, String> {
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
    paths: &[SendPathSpec],
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
    path: &SendPathSpec,
    files: &mut Vec<(String, Vec<u8>)>,
    limits: ClipboardLimits,
    total_bytes: u64,
    bytes_done: &mut u64,
    on_progress: &mut impl FnMut(u64, u64, &str),
) -> Result<(), String> {
    if !path.local.exists() {
        return Err(format!("path does not exist: {}", path.local.display()));
    }

    let metadata = tokio::fs::metadata(&path.local)
        .await
        .map_err(|error| format!("failed to stat {}: {error}", path.local.display()))?;

    if metadata.is_file() {
        read_single_file(path, files, limits, total_bytes, bytes_done, on_progress).await?;
        return Ok(());
    }

    if metadata.is_dir() {
        let mut entries = tokio::fs::read_dir(&path.local)
            .await
            .map_err(|error| format!("failed to read dir {}: {error}", path.local.display()))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|error| format!("failed to read dir entry in {}: {error}", path.local.display()))?
        {
            let entry_path = entry.path();
            if entry_path.is_file() {
                let relative = entry_path
                    .strip_prefix(&path.local)
                    .map_err(|error| format!("failed to relativize path: {error}"))?;
                let name = join_archive_path(&path.label, relative)?;
                read_single_file_by_name(
                    &name,
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
    path: &SendPathSpec,
    files: &mut Vec<(String, Vec<u8>)>,
    limits: ClipboardLimits,
    total_bytes: u64,
    bytes_done: &mut u64,
    on_progress: &mut impl FnMut(u64, u64, &str),
) -> Result<(), String> {
    read_single_file_by_name(
        &path.label,
        &path.local,
        files,
        limits,
        total_bytes,
        bytes_done,
        on_progress,
    )
    .await
}

async fn read_single_file_by_name(
    name: &str,
    path: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
    limits: ClipboardLimits,
    total_bytes: u64,
    bytes_done: &mut u64,
    on_progress: &mut impl FnMut(u64, u64, &str),
) -> Result<(), String> {
    on_progress(*bytes_done, total_bytes, name);

    let data = tokio::fs::read(path)
        .await
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    limits.validate_blob(data.len())?;
    *bytes_done = bytes_done.saturating_add(data.len() as u64);
    on_progress(*bytes_done, total_bytes, name);
    files.push((name.to_owned(), data));
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

    struct CurrentDirGuard {
        previous: PathBuf,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    fn change_current_dir(path: &Path) -> CurrentDirGuard {
        static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let lock = CWD_LOCK.lock().expect("cwd test lock");
        let previous = std::env::current_dir().expect("current dir");
        std::env::set_current_dir(path).expect("set current dir");
        CurrentDirGuard {
            previous,
            _lock: lock,
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.previous);
        }
    }

    #[test]
    fn preview_lists_files_in_directory() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("a.txt"), b"hello").unwrap();
        std::fs::write(temp.path().join("b.txt"), b"world!").unwrap();

        let spec = SendPathSpec {
            local: temp.path().to_path_buf(),
            label: "bundle".to_owned(),
        };
        let mut previews = collect_file_preview(&[spec]).unwrap();
        assert_eq!(previews.len(), 2);
        previews.sort_by(|left, right| left.name.cmp(&right.name));
        assert_eq!(previews[0].name, "bundle/a.txt");
        assert_eq!(previews[0].size, 5);
        assert_eq!(previews[1].name, "bundle/b.txt");
        assert_eq!(previews[1].size, 6);
    }

    #[test]
    fn resolves_relative_paths_from_current_directory() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("note.txt"), b"hello").unwrap();

        let _guard = change_current_dir(temp.path());
        let specs = prepare_send_paths(&[PathBuf::from("./note.txt")]).unwrap();

        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].label, "note.txt");
        assert!(specs[0].local.is_absolute());
        assert_eq!(
            std::fs::read_to_string(&specs[0].local).unwrap(),
            "hello"
        );
    }

    #[test]
    fn preserves_nested_relative_directory_label() {
        let temp = TempDir::new().unwrap();
        let nested = temp.path().join("proj").join("target");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("artifact.bin"), b"data").unwrap();

        let _guard = change_current_dir(temp.path());
        let specs = prepare_send_paths(&[PathBuf::from("proj/target")]).unwrap();

        let previews = collect_file_preview(&specs).unwrap();
        assert_eq!(previews.len(), 1);
        assert_eq!(previews[0].name, "proj/target/artifact.bin");
    }

    #[test]
    fn preserves_relative_file_label() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src").join("main.rs"), b"fn main() {}").unwrap();

        let _guard = change_current_dir(temp.path());
        let specs = prepare_send_paths(&[PathBuf::from("src/main.rs")]).unwrap();

        let previews = collect_file_preview(&specs).unwrap();
        assert_eq!(previews.len(), 1);
        assert_eq!(previews[0].name, "src/main.rs");
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

        let spec = SendPathSpec {
            local: path,
            label: "big.bin".to_owned(),
        };
        let result = read_files_from_paths(&[spec], limits, |_, _, _| {}).await;
        assert!(result.is_err());
    }
}
