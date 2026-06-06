use std::collections::HashSet;

use tokio::sync::RwLock;

use crate::device::DeviceId;
use crate::feature::FeatureId;

/// Account-scoped view of peer devices and feature delivery policy.
///
/// Populated by the daemon from the registration server. Features never
/// mutate or read this directly.
pub struct PeerDirectory {
    peers: RwLock<HashSet<DeviceId>>,
    disabled: RwLock<HashSet<(DeviceId, FeatureId)>>,
}

impl PeerDirectory {
    pub fn new() -> Self {
        Self {
            peers: RwLock::new(HashSet::new()),
            disabled: RwLock::new(HashSet::new()),
        }
    }

    pub async fn replace_peers(&self, peers: impl IntoIterator<Item = DeviceId>) {
        *self.peers.write().await = peers.into_iter().collect();
    }

    pub async fn peers(&self) -> Vec<DeviceId> {
        self.peers.read().await.iter().copied().collect()
    }

    pub async fn set_feature_enabled(&self, peer: DeviceId, feature: FeatureId, enabled: bool) {
        let mut disabled = self.disabled.write().await;
        let key = (peer, feature);
        if enabled {
            disabled.remove(&key);
        } else {
            disabled.insert(key);
        }
    }

    pub async fn is_feature_enabled(&self, peer: DeviceId, feature: FeatureId) -> bool {
        self.peers.read().await.contains(&peer)
            && !self.disabled.read().await.contains(&(peer, feature))
    }

    pub async fn recipients_for(&self, feature: FeatureId) -> Vec<DeviceId> {
        let peers = self.peers.read().await;
        let disabled = self.disabled.read().await;

        peers
            .iter()
            .copied()
            .filter(|peer| !disabled.contains(&(*peer, feature)))
            .collect()
    }
}

impl Default for PeerDirectory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn filters_disabled_feature_per_peer() {
        let directory = PeerDirectory::new();
        let laptop = DeviceId::new();
        let phone = DeviceId::new();

        directory.replace_peers([laptop, phone]).await;
        directory
            .set_feature_enabled(phone, FeatureId::Clipboard, false)
            .await;

        let recipients = directory.recipients_for(FeatureId::Clipboard).await;
        assert_eq!(recipients, vec![laptop]);
    }
}
