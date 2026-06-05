use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use failsafe_core::device::DeviceId;
use tokio::sync::Mutex;

use super::{BlobError, BlobHash, BlobTransfer};

#[derive(Default)]
struct MockState {
    blobs: HashMap<String, Vec<u8>>,
    collections: HashMap<String, Vec<(String, Vec<u8>)>>,
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
        state
            .collections
            .insert(root.as_str().to_owned(), files);
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
        _peer: DeviceId,
        root: &BlobHash,
    ) -> Result<Vec<(String, Vec<u8>)>, BlobError> {
        self.state
            .lock()
            .await
            .collections
            .get(root.as_str())
            .cloned()
            .ok_or_else(|| BlobError::NotFound(root.as_str().to_owned()))
    }
}
