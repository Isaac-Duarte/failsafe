use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_core::peer_address::PeerAddressBook;
use serde::{Deserialize, Serialize};

use crate::error::DaemonError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TransportKind {
    #[default]
    Mock,
    Iroh,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub device_id: DeviceId,
    #[serde(default)]
    pub peers: Vec<DeviceId>,
    #[serde(default)]
    pub enabled_features: Vec<FeatureId>,
    #[serde(default)]
    pub transport: TransportKind,
    #[serde(default)]
    pub peer_addresses: HashMap<DeviceId, String>,
}

impl Config {
    pub fn new(device_id: DeviceId) -> Self {
        Self {
            device_id,
            peers: Vec::new(),
            enabled_features: vec![FeatureId::Clipboard],
            transport: TransportKind::Mock,
            peer_addresses: HashMap::new(),
        }
    }

    pub fn peer_address_book(&self) -> PeerAddressBook {
        PeerAddressBook::from_map(self.peer_addresses.clone())
    }

    pub fn default_secret_key_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("failsafe").join("iroh.key"))
    }

    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("failsafe").join("config.toml"))
    }

    pub fn load(path: &Path) -> Result<Self, DaemonError> {
        let contents = std::fs::read_to_string(path).map_err(DaemonError::Io)?;
        toml::from_str(&contents).map_err(|error| {
            DaemonError::Config(format!("failed to parse {}: {error}", path.display()))
        })
    }

    pub fn save(&self, path: &Path) -> Result<(), DaemonError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(DaemonError::Io)?;
        }

        let contents = toml::to_string_pretty(self)
            .map_err(|error| DaemonError::Config(format!("failed to serialize config: {error}")))?;
        std::fs::write(path, contents).map_err(DaemonError::Io)
    }

    pub fn load_or_create(path: &Path) -> Result<Self, DaemonError> {
        if path.exists() {
            return Self::load(path);
        }

        let config = Self::new(DeviceId::new());
        config.save(path)?;
        Ok(config)
    }

    pub fn enabled_feature_set(&self) -> HashSet<FeatureId> {
        self.enabled_features.iter().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_toml() {
        let config = Config::new(DeviceId::new());
        let parsed: Config = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        assert_eq!(config.device_id, parsed.device_id);
        assert_eq!(config.enabled_features, parsed.enabled_features);
    }
}
