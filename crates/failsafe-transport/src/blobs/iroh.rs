use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use failsafe_core::device::DeviceId;
use iroh::Endpoint;
use iroh::EndpointId;
use iroh_blobs::api::Store;
use iroh_blobs::api::downloader::Shuffled;
use iroh_blobs::format::collection::Collection;
use iroh_blobs::store::fs::FsStore;
use iroh_blobs::{Hash, HashAndFormat};

use crate::iroh::SharedAddressState;

use super::{BlobError, BlobHash, BlobTransfer};

pub struct IrohBlobTransfer {
    store: FsStore,
    endpoint: Endpoint,
    address_state: SharedAddressState,
}

impl IrohBlobTransfer {
    pub fn new(store: FsStore, endpoint: Endpoint, address_state: SharedAddressState) -> Arc<Self> {
        Arc::new(Self {
            store,
            endpoint,
            address_state,
        })
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    fn parse_hash(hash: &BlobHash) -> Result<Hash, BlobError> {
        Hash::from_str(hash.as_str()).map_err(|error| BlobError::InvalidHash(error.to_string()))
    }

    fn peer_endpoint_id(&self, peer: DeviceId) -> Result<EndpointId, BlobError> {
        let state = self
            .address_state
            .read()
            .map_err(|error| BlobError::Store(format!("address state lock poisoned: {error}")))?;
        let address = state
            .book
            .get(peer)
            .ok_or_else(|| BlobError::PeerNotFound(peer.to_string()))?;
        let public_key = iroh::PublicKey::from_str(address).map_err(|error| {
            BlobError::Store(format!("invalid peer iroh address for {peer}: {error}"))
        })?;
        let endpoint_addr = iroh::EndpointAddr::new(public_key);
        Ok(endpoint_addr.id)
    }

    async fn download(&self, peer: DeviceId, request: HashAndFormat) -> Result<(), BlobError> {
        let provider = self.peer_endpoint_id(peer)?;
        self.store
            .downloader(&self.endpoint)
            .download(request, Shuffled::new(vec![provider]))
            .await
            .map_err(|error| BlobError::Store(error.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl BlobTransfer for IrohBlobTransfer {
    async fn store_bytes(&self, data: Vec<u8>) -> Result<BlobHash, BlobError> {
        let tag = self
            .store
            .add_bytes(Bytes::from(data))
            .await
            .map_err(|error| BlobError::Store(error.to_string()))?;
        self.store
            .tags()
            .create(tag.hash_and_format())
            .await
            .map_err(|error| BlobError::Store(error.to_string()))?;
        Ok(BlobHash(tag.hash.to_hex()))
    }

    async fn store_files(&self, files: Vec<(String, Vec<u8>)>) -> Result<BlobHash, BlobError> {
        let batch = self
            .store
            .batch()
            .await
            .map_err(|error| BlobError::Store(error.to_string()))?;

        let mut entries = Vec::with_capacity(files.len());
        for (name, data) in files {
            let tag = batch
                .add_bytes(Bytes::from(data))
                .await
                .map_err(|error| BlobError::Store(error.to_string()))?;
            entries.push((name, tag.hash()));
        }

        let collection = Collection::from_iter(entries);
        let root = collection
            .store(&self.store)
            .await
            .map_err(|error| BlobError::Store(error.to_string()))?;
        self.store
            .tags()
            .create(root.hash_and_format())
            .await
            .map_err(|error| BlobError::Store(error.to_string()))?;
        Ok(BlobHash(root.hash().to_hex())) // TempTag has hash() method
    }

    async fn fetch_bytes(&self, peer: DeviceId, hash: &BlobHash) -> Result<Vec<u8>, BlobError> {
        let hash = Self::parse_hash(hash)?;
        self.download(peer, HashAndFormat::raw(hash)).await?;
        let data = self
            .store
            .get_bytes(hash)
            .await
            .map_err(|error| BlobError::Store(error.to_string()))?;
        Ok(data.to_vec())
    }

    async fn fetch_collection_files(
        &self,
        peer: DeviceId,
        root: &BlobHash,
    ) -> Result<Vec<(String, Vec<u8>)>, BlobError> {
        let root_hash = Self::parse_hash(root)?;
        self.download(peer, HashAndFormat::hash_seq(root_hash))
            .await?;

        let collection = Collection::load(root_hash, self.store.deref())
            .await
            .map_err(|error| BlobError::Store(error.to_string()))?;

        let mut files = Vec::with_capacity(collection.len());
        for (name, hash) in collection.iter() {
            let data = self
                .store
                .get_bytes(*hash)
                .await
                .map_err(|error| BlobError::Store(error.to_string()))?;
            files.push((name.clone(), data.to_vec()));
        }

        Ok(files)
    }
}
