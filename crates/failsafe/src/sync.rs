use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

use failsafe_core::api::DeviceInfo;
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_core::peer::PeerDirectory;
use failsafe_core::peer_address::PeerAddressBook;
use failsafe_core::registry::FeatureRegistry;
use failsafe_transport::peer_updater::PeerAddressUpdater;

use crate::config::Config;
use crate::error::DaemonError;

pub fn find_self_device(self_id: DeviceId, devices: &[DeviceInfo]) -> Option<&DeviceInfo> {
    devices.iter().find(|device| device.device_id == self_id)
}

pub fn peer_address_book_from_devices(
    self_id: DeviceId,
    devices: &[DeviceInfo],
) -> PeerAddressBook {
    let mut addresses = HashMap::new();
    for device in devices {
        if device.device_id == self_id {
            continue;
        }
        addresses.insert(device.device_id, device.iroh_public_key.clone());
    }
    PeerAddressBook::from_map(addresses)
}

pub async fn apply_self_from_server(
    self_id: DeviceId,
    devices: &[DeviceInfo],
    device_name: &mut String,
    local_features: &mut HashSet<FeatureId>,
    config: &mut Config,
    config_path: &Path,
    registry: &mut FeatureRegistry,
    start_new_features: bool,
) -> Result<bool, DaemonError> {
    let Some(self_device) = find_self_device(self_id, devices) else {
        return Ok(false);
    };

    let server_features: HashSet<_> = self_device.enabled_features.iter().copied().collect();
    let name_changed = self_device.name != *device_name;
    let features_changed = server_features != *local_features;
    let policy_changed = name_changed || features_changed;

    if name_changed {
        *device_name = self_device.name.clone();
    }

    if features_changed {
        *local_features = server_features;
        sync_features_to_registry(local_features, registry, start_new_features).await?;
    }

    if policy_changed {
        config.apply_server_policy(
            &self_device.name,
            &self_device.enabled_features,
            config_path,
        )?;
    }

    Ok(policy_changed)
}

async fn sync_features_to_registry(
    target: &HashSet<FeatureId>,
    registry: &mut FeatureRegistry,
    start_new_features: bool,
) -> Result<(), DaemonError> {
    for feature in FeatureId::all() {
        if !registry.is_registered(*feature) {
            continue;
        }

        let should_enable = target.contains(feature);
        let is_enabled = registry.is_enabled(*feature);

        if should_enable && !is_enabled {
            if start_new_features {
                registry
                    .enable_and_start(*feature)
                    .await
                    .map_err(DaemonError::Feature)?;
            } else {
                registry.enable(*feature).map_err(DaemonError::Feature)?;
            }
        } else if !should_enable && is_enabled {
            registry.disable(*feature).await;
        }
    }

    Ok(())
}

pub async fn apply_server_devices(
    self_id: DeviceId,
    local_features: &HashSet<FeatureId>,
    devices: &[DeviceInfo],
    peers: &PeerDirectory,
    peer_updater: &dyn PeerAddressUpdater,
) {
    let mut peer_ids = Vec::new();
    let mut addresses = HashMap::new();

    for device in devices {
        if device.device_id == self_id {
            continue;
        }

        peer_ids.push(device.device_id);
        addresses.insert(device.device_id, device.iroh_public_key.clone());

        let remote_features: HashSet<_> = device.enabled_features.iter().copied().collect();
        for feature in FeatureId::all() {
            let enabled = local_features.contains(feature) && remote_features.contains(feature);
            peers
                .set_feature_enabled(device.device_id, *feature, enabled)
                .await;
        }
    }

    peers.replace_peers(peer_ids).await;
    peer_updater.update_peer_addresses(PeerAddressBook::from_map(addresses));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use failsafe_core::api::DeviceInfo;
    use failsafe_core::feature::{Feature, FeatureError};
    use failsafe_core::message::FeatureMessage;
    use failsafe_core::peer::PeerDirectory;
    use failsafe_core::registry::FeatureRegistry;
    use failsafe_transport::mock::MockTransport;

    use super::*;
    use crate::config::Config;

    struct EchoFeature;

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

        async fn handle_message(&mut self, _message: FeatureMessage) -> Result<(), FeatureError> {
            Ok(())
        }
    }

    fn test_registry() -> FeatureRegistry {
        let mut registry = FeatureRegistry::new();
        registry.register(Box::new(EchoFeature)).unwrap();
        registry
    }

    #[tokio::test]
    async fn applies_intersection_feature_policy() {
        let self_id = DeviceId::new();
        let peer = DeviceId::new();
        let peers = Arc::new(PeerDirectory::new());
        let transport = MockTransport::pair().await.0;
        let local_features = HashSet::from([FeatureId::Clipboard]);

        let devices = vec![DeviceInfo {
            device_id: peer,
            name: "peer".to_owned(),
            iroh_public_key: "abc".to_owned(),
            enabled_features: vec![],
            last_seen: None,
            online: false,
        }];

        apply_server_devices(self_id, &local_features, &devices, &peers, &transport).await;

        assert_eq!(peers.peers().await, vec![peer]);
        assert!(peers.recipients_for(FeatureId::Clipboard).await.is_empty());
    }

    #[tokio::test]
    async fn enables_feature_when_both_sides_support_it() {
        let self_id = DeviceId::new();
        let peer = DeviceId::new();
        let peers = Arc::new(PeerDirectory::new());
        let transport = MockTransport::pair().await.0;
        let local_features = HashSet::from([FeatureId::Clipboard]);

        let devices = vec![DeviceInfo {
            device_id: peer,
            name: "peer".to_owned(),
            iroh_public_key: "abc".to_owned(),
            enabled_features: vec![FeatureId::Clipboard],
            last_seen: None,
            online: false,
        }];

        apply_server_devices(self_id, &local_features, &devices, &peers, &transport).await;

        assert_eq!(peers.recipients_for(FeatureId::Clipboard).await, vec![peer]);
    }

    #[tokio::test]
    async fn apply_self_updates_daemon_policy_from_server() {
        let self_id = DeviceId::new();
        let mut config = Config::new(self_id);
        let config_path = std::env::temp_dir().join("failsafe-test-config.toml");
        let mut device_name = config.device_name.clone();
        let mut local_features = config.enabled_feature_set();
        let mut registry = test_registry();

        let devices = vec![DeviceInfo {
            device_id: self_id,
            name: "server-name".to_owned(),
            iroh_public_key: "abc".to_owned(),
            enabled_features: vec![],
            last_seen: None,
            online: true,
        }];

        let changed = apply_self_from_server(
            self_id,
            &devices,
            &mut device_name,
            &mut local_features,
            &mut config,
            &config_path,
            &mut registry,
            false,
        )
        .await
        .unwrap();

        assert!(changed);
        assert_eq!(device_name, "server-name");
        assert!(local_features.is_empty());
        assert!(!registry.is_enabled(FeatureId::Clipboard));
    }

    #[tokio::test]
    async fn apply_self_hot_reloads_new_features() {
        let self_id = DeviceId::new();
        let mut config = Config::new(self_id);
        config.enabled_features = vec![];
        let config_path = std::env::temp_dir().join("failsafe-test-config-hot.toml");
        let mut device_name = config.device_name.clone();
        let mut local_features = HashSet::new();
        let mut registry = test_registry();

        let devices = vec![DeviceInfo {
            device_id: self_id,
            name: "server-name".to_owned(),
            iroh_public_key: "abc".to_owned(),
            enabled_features: vec![FeatureId::Clipboard],
            last_seen: None,
            online: true,
        }];

        apply_self_from_server(
            self_id,
            &devices,
            &mut device_name,
            &mut local_features,
            &mut config,
            &config_path,
            &mut registry,
            true,
        )
        .await
        .unwrap();

        assert!(registry.is_enabled(FeatureId::Clipboard));
        assert!(local_features.contains(&FeatureId::Clipboard));
    }

    #[tokio::test]
    async fn apply_self_enables_shell_in_policy_without_registry_entry() {
        let self_id = DeviceId::new();
        let mut config = Config::new(self_id);
        let config_path = std::env::temp_dir().join("failsafe-test-config-shell.toml");
        let mut device_name = config.device_name.clone();
        let mut local_features = HashSet::new();
        let mut registry = test_registry();

        let devices = vec![DeviceInfo {
            device_id: self_id,
            name: "server-name".to_owned(),
            iroh_public_key: "abc".to_owned(),
            enabled_features: vec![FeatureId::Clipboard, FeatureId::Shell],
            last_seen: None,
            online: true,
        }];

        apply_self_from_server(
            self_id,
            &devices,
            &mut device_name,
            &mut local_features,
            &mut config,
            &config_path,
            &mut registry,
            true,
        )
        .await
        .unwrap();

        assert!(local_features.contains(&FeatureId::Shell));
        assert!(!registry.is_registered(FeatureId::Shell));
        assert!(registry.is_enabled(FeatureId::Clipboard));
    }
}
