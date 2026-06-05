use std::collections::HashSet;
use std::path::{Path, PathBuf};

use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use serde::{Deserialize, Serialize};

use crate::error::DaemonError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    Mock,
    #[default]
    Iroh,
}

fn default_device_name() -> String {
    "my-device".to_owned()
}

fn default_server_url() -> String {
    "http://127.0.0.1:8080".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub device_id: DeviceId,
    #[serde(default = "default_device_name")]
    pub device_name: String,
    #[serde(default = "default_server_url")]
    pub server_url: String,
    #[serde(default)]
    pub enabled_features: Vec<FeatureId>,
    #[serde(default)]
    pub transport: TransportKind,
}

impl Config {
    pub fn new(device_id: DeviceId) -> Self {
        Self {
            device_id,
            device_name: default_device_name(),
            server_url: default_server_url(),
            enabled_features: vec![FeatureId::Clipboard],
            transport: TransportKind::Iroh,
        }
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

    pub fn normalize_server_url(url: &str) -> String {
        url.trim().trim_end_matches('/').to_owned()
    }

    /// Apply a CLI/env override and persist when the value changes.
    pub fn apply_server_url_override(
        &mut self,
        path: &Path,
        override_url: Option<String>,
    ) -> Result<(), DaemonError> {
        let Some(url) = override_url else {
            return Ok(());
        };

        let normalized = Self::normalize_server_url(&url);
        if normalized.is_empty() {
            return Err(DaemonError::Config("server_url cannot be empty".to_owned()));
        }

        if self.server_url != normalized {
            self.server_url = normalized;
            self.save(path)?;
        }

        Ok(())
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
        assert_eq!(config.server_url, parsed.server_url);
    }
}
