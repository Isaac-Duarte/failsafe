mod error;
mod mock;

#[cfg(feature = "iroh-blobs")]
mod iroh;

pub use error::BlobError;
pub use mock::MockBlobTransfer;

#[cfg(feature = "iroh-blobs")]
pub use iroh::IrohBlobTransfer;

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use failsafe_core::device::DeviceId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlobHash(pub String);

impl BlobHash {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for BlobHash {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for BlobHash {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedFile {
    pub name: String,
    pub size: u64,
}

#[async_trait]
pub trait BlobTransfer: Send + Sync {
    async fn store_bytes(&self, data: Vec<u8>) -> Result<BlobHash, BlobError>;

    async fn store_files(&self, files: Vec<(String, Vec<u8>)>) -> Result<BlobHash, BlobError>;

    async fn fetch_bytes(&self, peer: DeviceId, hash: &BlobHash) -> Result<Vec<u8>, BlobError>;

    async fn fetch_collection_files(
        &self,
        peer: DeviceId,
        root: &BlobHash,
    ) -> Result<Vec<(String, Vec<u8>)>, BlobError>;

    /// Stream files from disk into the blob store and return a collection hash.
    async fn import_sources(
        &self,
        sources: &[(String, PathBuf)],
        progress: &mut (dyn FnMut(BlobProgress) + Send),
    ) -> Result<(BlobHash, Vec<ImportedFile>), BlobError>;

    /// Download a collection from a peer, resuming partial local state when present.
    async fn download_collection(
        &self,
        peer: DeviceId,
        root: &BlobHash,
        total_bytes: u64,
        progress: &mut (dyn FnMut(BlobProgress) + Send),
    ) -> Result<(), BlobError>;

    /// Export a complete local collection to a directory without loading files into RAM.
    async fn export_collection(
        &self,
        root: &BlobHash,
        dest_dir: &Path,
        progress: &mut (dyn FnMut(BlobProgress) + Send),
    ) -> Result<Vec<PathBuf>, BlobError>;

    /// Returns (bytes_present, bytes_total_hint, complete) for a collection root hash.
    async fn collection_status(
        &self,
        root: &BlobHash,
        total_bytes: u64,
    ) -> Result<(u64, u64, bool), BlobError>;
}
