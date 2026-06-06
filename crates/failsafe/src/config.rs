use std::collections::HashSet;
use std::path::{Path, PathBuf};

use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use serde::{Deserialize, Serialize};

use crate::error::DaemonError;

fn default_device_name() -> String {
    "my-device".to_owned()
}

fn default_server_url() -> String {
    "http://127.0.0.1:8080".to_owned()
}

fn default_clipboard_max_file_bytes() -> u64 {
    100 * 1024 * 1024
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub device_id: DeviceId,
    /// Cached copy of the server-managed device name.
    #[serde(default = "default_device_name")]
    pub device_name: String,
    #[serde(default = "default_server_url")]
    pub server_url: String,
    /// Cached copy of the server-managed enabled features.
    #[serde(default)]
    pub enabled_features: Vec<FeatureId>,
    #[serde(default)]
    pub blob_store_path: Option<PathBuf>,
    #[serde(default = "default_clipboard_max_file_bytes")]
    pub clipboard_max_file_bytes: u64,
}

impl Config {
    pub fn new(device_id: DeviceId) -> Self {
        Self {
            device_id,
            device_name: default_device_name(),
            server_url: default_server_url(),
            enabled_features: vec![FeatureId::Clipboard, FeatureId::Shell],
            blob_store_path: None,
            clipboard_max_file_bytes: default_clipboard_max_file_bytes(),
        }
    }

    pub fn default_secret_key_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("failsafe").join("iroh.key"))
    }

    pub fn default_blob_store_path() -> Option<PathBuf> {
        dirs::data_local_dir().map(|dir| dir.join("failsafe").join("blobs"))
    }

    pub fn resolved_blob_store_path(&self) -> Option<PathBuf> {
        self.blob_store_path
            .clone()
            .or_else(Self::default_blob_store_path)
    }

    pub fn clipboard_limits(&self) -> failsafe_clipboard::limits::ClipboardLimits {
        failsafe_clipboard::limits::ClipboardLimits {
            max_file_bytes: self.clipboard_max_file_bytes,
            max_total_bytes: self.clipboard_max_file_bytes.saturating_mul(5),
        }
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
    /// Update the local cache of server-managed device policy.
    pub fn apply_server_policy(
        &mut self,
        name: &str,
        features: &[FeatureId],
        path: &Path,
    ) -> Result<bool, DaemonError> {
        let changed = self.device_name != name || self.enabled_features != features;
        if changed {
            self.device_name = name.to_owned();
            self.enabled_features = features.to_vec();
            self.save(path)?;
        }
        Ok(changed)
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

    #[test]
    fn apply_server_policy_persists_server_managed_fields() {
        let dir = std::env::temp_dir().join(format!("failsafe-config-test-{}", DeviceId::new()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");

        let mut config = Config::new(DeviceId::new());
        config.save(&path).unwrap();

        let changed = config.apply_server_policy("renamed", &[], &path).unwrap();
        assert!(changed);

        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.device_name, "renamed");
        assert!(loaded.enabled_features.is_empty());

        let _ = std::fs::remove_dir_all(dir);
    }
}
