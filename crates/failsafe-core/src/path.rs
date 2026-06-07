use std::path::{Path, PathBuf};

/// Returns the basename of `name`, or `"file"` when missing or empty.
pub fn sanitize_filename(name: &str) -> String {
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

/// Writes `files` into `base`, creating the directory when needed.
pub async fn write_named_files(
    base: &Path,
    files: &[(String, Vec<u8>)],
) -> Result<Vec<PathBuf>, std::io::Error> {
    tokio::fs::create_dir_all(base).await?;

    let mut paths = Vec::with_capacity(files.len());
    for (name, data) in files {
        let path = base.join(sanitize_filename(name));
        tokio::fs::write(&path, data).await?;
        paths.push(path);
    }

    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_filename_uses_basename() {
        assert_eq!(sanitize_filename("../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename(""), "file");
    }

    #[tokio::test]
    async fn write_named_files_creates_entries() {
        let dir = std::env::temp_dir().join(format!("failsafe-path-{}", uuid::Uuid::new_v4()));
        let paths = write_named_files(
            &dir,
            &[("hello.txt".to_owned(), b"hello".to_vec())],
        )
        .await
        .unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].exists());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
