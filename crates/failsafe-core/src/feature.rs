use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::message::FeatureMessage;

/// Identifies a sync feature
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureId {
    Clipboard,
}

impl FeatureId {
    pub fn all() -> &'static [FeatureId] {
        &[FeatureId::Clipboard]
    }
}

impl fmt::Display for FeatureId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clipboard => write!(f, "clipboard"),
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("unknown feature `{0}`")]
pub struct UnknownFeatureId(pub String);

impl FromStr for FeatureId {
    type Err = UnknownFeatureId;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "clipboard" => Ok(Self::Clipboard),
            other => Err(UnknownFeatureId(other.to_owned())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FeatureError {
    #[error("feature `{0}` is already registered")]
    AlreadyRegistered(FeatureId),

    #[error("feature `{0}` is not registered")]
    NotRegistered(FeatureId),

    #[error("feature `{0}` is not enabled")]
    NotEnabled(FeatureId),

    #[error("feature `{0}` failed: {1}")]
    Failed(FeatureId, String),
}

/// A pluggable sync capability that can be enabled per device.
#[async_trait]
pub trait Feature: Send + Sync {
    fn id(&self) -> FeatureId;

    async fn start(&mut self) -> Result<(), FeatureError>;

    async fn stop(&mut self) -> Result<(), FeatureError>;

    async fn handle_message(&mut self, message: FeatureMessage) -> Result<(), FeatureError>;
}
