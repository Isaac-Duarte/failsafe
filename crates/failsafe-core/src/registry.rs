use std::collections::{HashMap, HashSet};

use crate::feature::{Feature, FeatureError, FeatureId};
use crate::message::FeatureMessage;

/// Holds registered features and tracks which are enabled on this device.
pub struct FeatureRegistry {
    features: HashMap<FeatureId, Box<dyn Feature>>,
    enabled: HashSet<FeatureId>,
}

impl FeatureRegistry {
    pub fn new() -> Self {
        Self {
            features: HashMap::new(),
            enabled: HashSet::new(),
        }
    }

    pub fn register(&mut self, feature: Box<dyn Feature>) -> Result<(), FeatureError> {
        let id = feature.id();
        if self.features.contains_key(&id) {
            return Err(FeatureError::AlreadyRegistered(id));
        }
        self.features.insert(id, feature);
        Ok(())
    }

    pub fn enable(&mut self, id: FeatureId) -> Result<(), FeatureError> {
        if !self.features.contains_key(&id) {
            return Err(FeatureError::NotRegistered(id));
        }
        self.enabled.insert(id);
        Ok(())
    }

    pub async fn disable(&mut self, id: FeatureId) {
        self.enabled.remove(&id);
        if let Some(feature) = self.features.get_mut(&id) {
            let _ = feature.stop().await;
        }
    }

    pub fn is_enabled(&self, id: FeatureId) -> bool {
        self.enabled.contains(&id)
    }

    pub fn enabled_features(&self) -> impl Iterator<Item = FeatureId> + '_ {
        self.enabled.iter().copied()
    }

    pub async fn start_enabled(&mut self) -> Result<(), FeatureError> {
        for id in self.enabled.clone() {
            let feature = self
                .features
                .get_mut(&id)
                .expect("enabled feature must be registered");
            feature.start().await?;
        }
        Ok(())
    }

    pub async fn dispatch(&mut self, message: FeatureMessage) -> Result<(), FeatureError> {
        let id = message.feature;
        if !self.is_enabled(id) {
            return Err(FeatureError::NotEnabled(id));
        }
        let feature = self
            .features
            .get_mut(&id)
            .ok_or(FeatureError::NotRegistered(id))?;
        feature.handle_message(message).await
    }
}

impl Default for FeatureRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::device::DeviceId;

    struct EchoFeature {
        last_payload: Option<Vec<u8>>,
    }

    impl EchoFeature {
        fn new() -> Self {
            Self {
                last_payload: None,
            }
        }
    }

    #[async_trait]
    impl Feature for EchoFeature {
        fn id(&self) -> FeatureId {
            FeatureId::Clipboard
        }

        async fn start(&mut self) -> Result<(), FeatureError> {
            Ok(())
        }

        async fn stop(&mut self) -> Result<(), FeatureError> {
            Ok(())
        }

        async fn handle_message(&mut self, message: FeatureMessage) -> Result<(), FeatureError> {
            self.last_payload = Some(message.payload);
            Ok(())
        }
    }

    #[tokio::test]
    async fn register_enable_and_dispatch() {
        let mut registry = FeatureRegistry::new();
        let feature = Box::new(EchoFeature::new());
        registry.register(feature).unwrap();
        registry.enable(FeatureId::Clipboard).unwrap();

        let from = DeviceId::new();
        let to = DeviceId::new();
        registry
            .dispatch(FeatureMessage::new(
                from,
                to,
                FeatureId::Clipboard,
                b"hello",
            ))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn dispatch_rejects_disabled_feature() {
        let mut registry = FeatureRegistry::new();
        registry.register(Box::new(EchoFeature::new())).unwrap();

        let err = registry
            .dispatch(FeatureMessage::new(
                DeviceId::new(),
                DeviceId::new(),
                FeatureId::Clipboard,
                b"hello",
            ))
            .await
            .unwrap_err();

        assert!(matches!(err, FeatureError::NotEnabled(FeatureId::Clipboard)));
    }
}
