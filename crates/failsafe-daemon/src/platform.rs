use std::path::PathBuf;
use std::sync::OnceLock;

/// Platform-specific storage paths. When unset, falls back to `dirs` (desktop).
#[derive(Debug, Clone)]
pub struct PlatformPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub downloads_dir: PathBuf,
}

static PLATFORM_PATHS: OnceLock<PlatformPaths> = OnceLock::new();

impl PlatformPaths {
    pub fn install(paths: PlatformPaths) -> Result<(), PlatformPaths> {
        PLATFORM_PATHS.set(paths)
    }

    pub fn get() -> Option<&'static PlatformPaths> {
        PLATFORM_PATHS.get()
    }

    pub fn config_root() -> Option<PathBuf> {
        Self::get()
            .map(|paths| paths.config_dir.clone())
            .or_else(|| dirs::config_dir().map(|dir| dir.join("failsafe")))
    }

    pub fn data_root() -> Option<PathBuf> {
        Self::get()
            .map(|paths| paths.data_dir.clone())
            .or_else(|| dirs::data_local_dir().map(|dir| dir.join("failsafe")))
    }

    pub fn downloads_root() -> Option<PathBuf> {
        Self::get().map(|paths| paths.downloads_dir.clone()).or_else(|| {
            dirs::download_dir()
                .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
                .map(|dir| dir.join("failsafe"))
        })
    }
}
