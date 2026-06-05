use serde::{Deserialize, Serialize};

use crate::device::DeviceId;
use crate::feature::FeatureId;

pub const FEATURE_MESSAGE_VERSION: u32 = 1;

/// Envelope for messages exchanged between devices over a transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureMessage {
    pub version: u32,
    pub from: DeviceId,
    pub to: DeviceId,
    pub feature: FeatureId,
    pub payload: Vec<u8>,
}

impl FeatureMessage {
    pub fn new(
        from: DeviceId,
        to: DeviceId,
        feature: FeatureId,
        payload: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            version: FEATURE_MESSAGE_VERSION,
            from,
            to,
            feature,
            payload: payload.into(),
        }
    }
}
