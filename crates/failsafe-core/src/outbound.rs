use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::feature::FeatureId;

/// A feature-local event to be routed to peer devices by the runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub feature: FeatureId,
    pub payload: Vec<u8>,
}

impl OutboundMessage {
    pub fn new(feature: FeatureId, payload: impl Into<Vec<u8>>) -> Self {
        Self {
            feature,
            payload: payload.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error("publish failed: {0}")]
    Failed(String),
}

/// Publishes outbound feature events without knowledge of peers or transport.
#[async_trait]
pub trait OutboundPublisher: Send + Sync {
    async fn publish(&self, message: OutboundMessage) -> Result<(), PublishError>;
}
