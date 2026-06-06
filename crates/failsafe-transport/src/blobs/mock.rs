use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use failsafe_core::device::DeviceId;
use tokio::sync::{Mutex, Semaphore};

use super::{
    BlobError, BlobHash, BlobProgress, BlobTransfer, ImportedFile, MAX_CONCURRENT_IMPORTS,
};

#[derive(Default)]
struct MockState {
    blobs: HashMap<String, Vec<u8>>,
    collections: HashMap<String, Vec<(String, Vec<u8>)>>,
    downloaded: HashSet<String>,
}

pub struct MockBlobTransfer {
    state: Arc<Mutex<MockState>>,
}

impl MockBlobTransfer {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockState::default())),
        }
    }
}

impl Default for MockBlobTransfer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BlobTransfer for MockBlobTransfer {
    async fn store_bytes(&self, data: Vec<u8>) -> Result<BlobHash, BlobError> {
        let hash = BlobHash(hex::encode(blake3::hash(&data).as_bytes()));
        self.state
            .lock()
            .await
            .blobs
            .insert(hash.as_str().to_owned(), data);
        Ok(hash)
    }

    async fn store_files(&self, files: Vec<(String, Vec<u8>)>) -> Result<BlobHash, BlobError> {
        let mut state = self.state.lock().await;
        let root = BlobHash(hex::encode(
            blake3::hash(
                files
                    .iter()
                    .map(|(name, data)| {
                        format!("{name}:{}", hex::encode(blake3::hash(data).as_bytes()))
                    })
                    .collect::<Vec<_>>()
                    .join("|")
                    .as_bytes(),
            )
            .as_bytes(),
        ));
        state.collections.insert(root.as_str().to_owned(), files);
        Ok(root)
    }

    async fn fetch_bytes(&self, _peer: DeviceId, hash: &BlobHash) -> Result<Vec<u8>, BlobError> {
        self.state
            .lock()
            .await
            .blobs
            .get(hash.as_str())
            .cloned()
            .ok_or_else(|| BlobError::NotFound(hash.as_str().to_owned()))
    }

    async fn fetch_collection_files(
        &self,
        peer: DeviceId,
        root: &BlobHash,
    ) -> Result<Vec<(String, Vec<u8>)>, BlobError> {
        self.download_collection(peer, root, 0, &mut |_| {}).await?;
        self.state
            .lock()
            .await
            .collections
            .get(root.as_str())
            .cloned()
            .ok_or_else(|| BlobError::NotFound(root.as_str().to_owned()))
    }

    async fn import_sources(
        &self,
        sources: &[(String, PathBuf)],
        progress: &mut (dyn FnMut(BlobProgress) + Send),
    ) -> Result<(BlobHash, Vec<ImportedFile>), BlobError> {
        let total_bytes: u64 = sources
            .iter()
            .map(|(_, path)| {
                std::fs::metadata(path)
                    .map(|meta| meta.len())
                    .unwrap_or_default()
            })
            .sum();

        let import_slots = Arc::new(Semaphore::new(MAX_CONCURRENT_IMPORTS));
        let mut handles = Vec::with_capacity(sources.len());
        for (name, path) in sources {
            let name = name.clone();
            let path = path.clone();
            let import_slots = import_slots.clone();
            handles.push(tokio::spawn(async move {
                let Ok(_permit) = import_slots.acquire().await else {
                    return Err(BlobError::Store("import cancelled".to_owned()));
                };
                let data = tokio::fs::read(&path)
                    .await
                    .map_err(|error| BlobError::Store(error.to_string()))?;
                Ok::<_, BlobError>((name, data))
            }));
        }

        let mut files = Vec::with_capacity(sources.len());
        let mut bytes_done = 0u64;
        for handle in handles {
            let (name, data) = handle
                .await
                .map_err(|error| BlobError::Store(format!("import task failed: {error}")))??;
            bytes_done = bytes_done.saturating_add(data.len() as u64);
            progress(BlobProgress {
                bytes_done,
                bytes_total: total_bytes,
                current_file: Some(name.clone()),
            });
            files.push((name, data));
        }

        let imported: Vec<ImportedFile> = files
            .iter()
            .map(|(name, data)| ImportedFile {
                name: name.clone(),
                size: data.len() as u64,
            })
            .collect();
        let hash = self.store_files(files).await?;
        Ok((hash, imported))
    }

    async fn download_collection(
        &self,
        _peer: DeviceId,
        root: &BlobHash,
        total_bytes: u64,
        progress: &mut (dyn FnMut(BlobProgress) + Send),
    ) -> Result<(), BlobError> {
        if !self
            .state
            .lock()
            .await
            .collections
            .contains_key(root.as_str())
        {
            return Err(BlobError::NotFound(root.as_str().to_owned()));
        }
        self.state
            .lock()
            .await
            .downloaded
            .insert(root.as_str().to_owned());
        progress(BlobProgress {
            bytes_done: total_bytes,
            bytes_total: total_bytes,
            current_file: None,
        });
        Ok(())
    }

    async fn export_collection(
        &self,
        root: &BlobHash,
        dest_dir: &Path,
        progress: &mut (dyn FnMut(BlobProgress) + Send),
    ) -> Result<Vec<PathBuf>, BlobError> {
        let files = self
            .state
            .lock()
            .await
            .collections
            .get(root.as_str())
            .cloned()
            .ok_or_else(|| BlobError::NotFound(root.as_str().to_owned()))?;

        tokio::fs::create_dir_all(dest_dir)
            .await
            .map_err(|error| BlobError::Store(error.to_string()))?;

        let total = files.len() as u64;
        let mut paths = Vec::with_capacity(files.len());
        for (index, (name, data)) in files.iter().enumerate() {
            let path = dest_dir.join(name);
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|error| BlobError::Store(error.to_string()))?;
            }
            tokio::fs::write(&path, data)
                .await
                .map_err(|error| BlobError::Store(error.to_string()))?;
            progress(BlobProgress {
                bytes_done: index as u64 + 1,
                bytes_total: total,
                current_file: Some(name.clone()),
            });
            paths.push(path);
        }
        Ok(paths)
    }

    async fn collection_status(
        &self,
        root: &BlobHash,
        total_bytes: u64,
    ) -> Result<(u64, u64, bool), BlobError> {
        let state = self.state.lock().await;
        let complete = state.downloaded.contains(root.as_str())
            || state.collections.contains_key(root.as_str());
        let bytes_done = if complete { total_bytes } else { 0 };
        Ok((bytes_done, total_bytes, complete))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn import_many_files_with_limited_concurrency() {
        let temp = TempDir::new().expect("tempdir");
        let mut sources = Vec::new();
        for index in 0..64 {
            let name = format!("file-{index}.txt");
            let path = temp.path().join(&name);
            std::fs::write(&path, b"payload").expect("write fixture");
            sources.push((name, path));
        }

        let transfer = MockBlobTransfer::new();
        let (_, imported) = transfer
            .import_sources(&sources, &mut |_| {})
            .await
            .expect("import many files");

        assert_eq!(imported.len(), 64);
    }
}
