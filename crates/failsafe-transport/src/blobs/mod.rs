mod error;
mod mock;

#[cfg(feature = "iroh-blobs")]
mod iroh;

pub use error::BlobError;
pub use mock::MockBlobTransfer;

#[cfg(feature = "iroh-blobs")]
pub use iroh::IrohBlobTransfer;

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
}
