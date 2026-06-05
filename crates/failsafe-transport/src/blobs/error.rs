use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlobError {
    #[error("blob store error: {0}")]
    Store(String),

    #[error("peer `{0}` not found")]
    PeerNotFound(String),

    #[error("invalid blob hash: {0}")]
    InvalidHash(String),

    #[error("blob not found: {0}")]
    NotFound(String),
}
