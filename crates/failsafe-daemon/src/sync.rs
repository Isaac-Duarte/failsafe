use std::collections::HashMap;
use std::collections::HashSet;

use failsafe_core::api::DeviceInfo;
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_core::peer::PeerDirectory;
use failsafe_core::peer_address::PeerAddressBook;
use failsafe_transport::peer_updater::PeerAddressUpdater;

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
            let enabled =
                local_features.contains(feature) && remote_features.contains(feature);
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

    use failsafe_core::api::DeviceInfo;
    use failsafe_core::peer::PeerDirectory;
    use failsafe_transport::mock::MockTransport;

    use super::*;

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
        }];

        apply_server_devices(
            self_id,
            &local_features,
            &devices,
            &peers,
            &transport,
        )
        .await;

        assert_eq!(peers.peers().await, vec![peer]);
        assert!(
            peers
                .recipients_for(FeatureId::Clipboard)
                .await
                .is_empty()
        );
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
        }];

        apply_server_devices(
            self_id,
            &local_features,
            &devices,
            &peers,
            &transport,
        )
        .await;

        assert_eq!(
            peers.recipients_for(FeatureId::Clipboard).await,
            vec![peer]
        );
    }
}
