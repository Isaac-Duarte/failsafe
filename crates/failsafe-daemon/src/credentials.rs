use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::DaemonError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Credentials {
    pub auth_token: String,
}

impl Credentials {
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("failsafe").join("credentials.toml"))
    }

    pub fn load(path: &Path) -> Result<Self, DaemonError> {
        let contents = std::fs::read_to_string(path).map_err(DaemonError::Io)?;
        toml::from_str(&contents).map_err(|error| {
            DaemonError::Config(format!(
                "failed to parse credentials {}: {error}",
                path.display()
            ))
        })
    }

    pub fn save(&self, path: &Path) -> Result<(), DaemonError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(DaemonError::Io)?;
        }

        let contents = toml::to_string_pretty(self).map_err(|error| {
            DaemonError::Config(format!("failed to serialize credentials: {error}"))
        })?;
        std::fs::write(path, contents).map_err(DaemonError::Io)
    }

    pub fn load_or_error() -> Result<Self, DaemonError> {
        let path = Self::default_path().ok_or_else(|| {
            DaemonError::Config("could not determine credentials path for this platform".to_owned())
        })?;

        if !path.exists() {
            return Err(DaemonError::Config(format!(
                "credentials not found at {}; run `failsafe login` first",
                path.display()
            )));
        }

        Self::load(&path)
    }
}
