use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::control::{ControlError, ControlStream};
use crate::message::FeatureMessage;
use crate::peer::PeerDirectory;

/// Identifies a sync feature by stable string id (e.g. `"clipboard"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(transparent)]
pub struct FeatureId(pub String);

impl FeatureId {
    pub fn from_static(id: &'static str) -> Self {
        Self(id.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FeatureId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("unknown feature `{0}`")]
pub struct UnknownFeatureId(pub String);

impl FromStr for FeatureId {
    type Err = UnknownFeatureId;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.is_empty() {
            return Err(UnknownFeatureId(value.to_owned()));
        }
        Ok(Self(value.to_owned()))
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

/// Static metadata for a feature, implemented on a marker type.
pub trait FeatureSpec {
    fn id() -> &'static str;
    fn label() -> &'static str;
    fn description() -> &'static str;

    fn feature_id() -> FeatureId {
        FeatureId::from_static(Self::id())
    }
}

/// Shared read-only context passed to control handlers.
pub struct ControlContext<'a> {
    pub peers: &'a PeerDirectory,
    pub local_features: &'a HashSet<FeatureId>,
}

/// A pluggable sync capability that can be enabled per device.
#[async_trait]
pub trait Feature: Send + Sync {
    fn id(&self) -> FeatureId;

    async fn start(&mut self) -> Result<(), FeatureError>;

    async fn stop(&mut self) -> Result<(), FeatureError>;

    async fn handle_message(&mut self, message: FeatureMessage) -> Result<(), FeatureError>;
}

/// Handles CLI-initiated control requests for a feature.
#[async_trait]
pub trait FeatureControl: Send + Sync {
    fn control_id(&self) -> FeatureId;

    async fn handle_control(
        &self,
        ctx: &ControlContext<'_>,
        stream: ControlStream,
        body: serde_json::Value,
    ) -> Result<(), ControlError>;
}
