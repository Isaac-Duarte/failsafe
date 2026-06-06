use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use failsafe_core::device::DeviceId;
use iroh::Endpoint;
use iroh::EndpointId;
use iroh_blobs::api::blobs::{AddPathOptions, AddProgressItem, ExportMode, ExportOptions, ExportProgressItem, ImportMode};
use iroh_blobs::api::downloader::{DownloadProgressItem, Shuffled};
use iroh_blobs::api::Store;
use iroh_blobs::format::collection::Collection;
use iroh_blobs::store::fs::FsStore;
use iroh_blobs::{BlobFormat, Hash, HashAndFormat};
use n0_future::StreamExt;

use crate::iroh::SharedAddressState;

use super::{BlobError, BlobHash, BlobProgress, BlobTransfer, ImportedFile};

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

    fn export_target(base: &Path, name: &str) -> Result<PathBuf, BlobError> {
        let mut path = base.to_path_buf();
        for part in name.split('/') {
            if part.is_empty() || part == "." || part == ".." {
                return Err(BlobError::Store(format!("invalid path component: {part}")));
            }
            path.push(part);
        }
        Ok(path)
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
        Ok(BlobHash(root.hash().to_hex()))
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
        let mut bytes_done = 0u64;
        let mut imported = Vec::with_capacity(sources.len());
        let mut entries = Vec::with_capacity(sources.len());

        for (name, path) in sources {
            progress(BlobProgress {
                bytes_done,
                bytes_total: total_bytes,
                current_file: Some(name.clone()),
            });

            let import = self.store.add_path_with_opts(AddPathOptions {
                path: path.clone(),
                mode: ImportMode::TryReference,
                format: BlobFormat::Raw,
            });
            let mut stream = import.stream().await;
            let mut file_size = 0u64;
            let temp_tag = loop {
                let Some(item) = stream.next().await else {
                    return Err(BlobError::Store(format!(
                        "import stream ended before completion for {}",
                        path.display()
                    )));
                };
                match item {
                    AddProgressItem::Size(size) => file_size = size,
                    AddProgressItem::CopyProgress(offset) => {
                        progress(BlobProgress {
                            bytes_done: bytes_done.saturating_add(offset),
                            bytes_total: total_bytes,
                            current_file: Some(name.clone()),
                        });
                    }
                    AddProgressItem::Error(cause) => {
                        return Err(BlobError::Store(format!(
                            "failed to import {}: {cause}",
                            path.display()
                        )));
                    }
                    AddProgressItem::Done(tag) => break tag,
                    _ => {}
                }
            };

            bytes_done = bytes_done.saturating_add(file_size);
            entries.push((name.clone(), temp_tag.hash()));
            imported.push(ImportedFile {
                name: name.clone(),
                size: file_size,
            });
            progress(BlobProgress {
                bytes_done,
                bytes_total: total_bytes,
                current_file: Some(name.clone()),
            });
        }

        progress(BlobProgress {
            bytes_done: total_bytes,
            bytes_total: total_bytes,
            current_file: None,
        });

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

        Ok((BlobHash(root.hash().to_hex()), imported))
    }

    async fn download_collection(
        &self,
        peer: DeviceId,
        root: &BlobHash,
        total_bytes: u64,
        progress: &mut (dyn FnMut(BlobProgress) + Send),
    ) -> Result<(), BlobError> {
        let root_hash = Self::parse_hash(root)?;
        let hash_and_format = HashAndFormat::hash_seq(root_hash);
        let local = self
            .store
            .remote()
            .local(hash_and_format)
            .await
            .map_err(|error| BlobError::Store(error.to_string()))?;

        if local.is_complete() {
            progress(BlobProgress {
                bytes_done: total_bytes,
                bytes_total: total_bytes,
                current_file: None,
            });
            return Ok(());
        }

        let provider = self.peer_endpoint_id(peer)?;
        let mut stream = self
            .store
            .downloader(&self.endpoint)
            .download(hash_and_format, Shuffled::new(vec![provider]))
            .stream()
            .await
            .map_err(|error| BlobError::Store(error.to_string()))?;

        while let Some(item) = stream.next().await {
            match item {
                DownloadProgressItem::Progress(bytes) => {
                    progress(BlobProgress {
                        bytes_done: bytes.min(total_bytes),
                        bytes_total: total_bytes,
                        current_file: None,
                    });
                }
                DownloadProgressItem::DownloadError => {
                    return Err(BlobError::Store("download failed".to_owned()));
                }
                DownloadProgressItem::Error(error) => {
                    return Err(BlobError::Store(error.to_string()));
                }
                _ => {}
            }
        }

        let local = self
            .store
            .remote()
            .local(hash_and_format)
            .await
            .map_err(|error| BlobError::Store(error.to_string()))?;
        if !local.is_complete() {
            return Err(BlobError::Store(
                "download ended before collection was complete".to_owned(),
            ));
        }

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
        let root_hash = Self::parse_hash(root)?;
        let collection = Collection::load(root_hash, self.store.deref())
            .await
            .map_err(|error| BlobError::Store(error.to_string()))?;

        tokio::fs::create_dir_all(dest_dir)
            .await
            .map_err(|error| BlobError::Store(format!("failed to create export dir: {error}")))?;

        let total_files = collection.len() as u64;
        let mut paths = Vec::with_capacity(collection.len());

        for (index, (name, hash)) in collection.iter().enumerate() {
            let target = Self::export_target(dest_dir, name)?;
            if let Some(parent) = target.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|error| {
                    BlobError::Store(format!("failed to create parent dir: {error}"))
                })?;
            }
            if target.exists() {
                tracing::info!(
                    path = %target.display(),
                    "export target already exists, skipping"
                );
                paths.push(target);
                progress(BlobProgress {
                    bytes_done: index as u64 + 1,
                    bytes_total: total_files,
                    current_file: Some(name.clone()),
                });
                continue;
            }

            progress(BlobProgress {
                bytes_done: index as u64,
                bytes_total: total_files,
                current_file: Some(name.clone()),
            });

            let mut stream = self
                .store
                .export_with_opts(ExportOptions {
                    hash: *hash,
                    target: target.clone(),
                    mode: ExportMode::Copy,
                })
                .stream()
                .await;
            let mut file_size = 0u64;
            while let Some(item) = stream.next().await {
                match item {
                    ExportProgressItem::Size(size) => file_size = size,
                    ExportProgressItem::CopyProgress(offset) => {
                        progress(BlobProgress {
                            bytes_done: index as u64,
                            bytes_total: total_files,
                            current_file: Some(name.clone()),
                        });
                        let _ = offset;
                    }
                    ExportProgressItem::Error(cause) => {
                        return Err(BlobError::Store(format!(
                            "failed to export {name}: {cause}"
                        )));
                    }
                    ExportProgressItem::Done => break,
                }
            }

            let _ = file_size;
            paths.push(target);
        }

        progress(BlobProgress {
            bytes_done: total_files,
            bytes_total: total_files,
            current_file: None,
        });

        Ok(paths)
    }

    async fn collection_status(
        &self,
        root: &BlobHash,
        total_bytes: u64,
    ) -> Result<(u64, u64, bool), BlobError> {
        let root_hash = Self::parse_hash(root)?;
        let local = self
            .store
            .remote()
            .local(HashAndFormat::hash_seq(root_hash))
            .await
            .map_err(|error| BlobError::Store(error.to_string()))?;
        Ok((
            local.local_bytes(),
            total_bytes,
            local.is_complete(),
        ))
    }
}
