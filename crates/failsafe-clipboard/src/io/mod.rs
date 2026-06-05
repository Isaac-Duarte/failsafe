mod content;
mod files;
#[cfg(target_os = "linux")]
mod linux_files;
pub mod mock;
mod system;

use async_trait::async_trait;

pub use content::{ClipboardContent, ClipboardIoError, ImageDataOwned};
pub use files::{default_clipboard_cache_dir, write_received_files};

#[async_trait]
pub trait ClipboardIo: Send + Sync {
    async fn read(&self) -> Result<Option<ClipboardContent>, ClipboardIoError>;

    async fn write(&self, content: &ClipboardContent) -> Result<(), ClipboardIoError>;
}

pub struct SystemClipboardIo;

#[async_trait]
impl ClipboardIo for SystemClipboardIo {
    async fn read(&self) -> Result<Option<ClipboardContent>, ClipboardIoError> {
        tokio::task::spawn_blocking(system::read_system_clipboard)
            .await
            .map_err(|error| ClipboardIoError::Unavailable(error.to_string()))?
    }

    async fn write(&self, content: &ClipboardContent) -> Result<(), ClipboardIoError> {
        let content = content.clone();
        tokio::task::spawn_blocking(move || system::write_system_clipboard(&content))
            .await
            .map_err(|error| ClipboardIoError::Unavailable(error.to_string()))?
    }
}
