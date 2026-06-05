use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClipboardIoError {
    #[error("clipboard unavailable: {0}")]
    Unavailable(String),
}

#[async_trait]
pub trait ClipboardIo: Send + Sync {
    async fn get_text(&self) -> Result<Option<String>, ClipboardIoError>;

    async fn set_text(&self, text: &str) -> Result<(), ClipboardIoError>;
}

pub struct SystemClipboardIo;

#[async_trait]
impl ClipboardIo for SystemClipboardIo {
    async fn get_text(&self) -> Result<Option<String>, ClipboardIoError> {
        tokio::task::spawn_blocking(read_system_text)
            .await
            .map_err(|error| ClipboardIoError::Unavailable(error.to_string()))?
    }

    async fn set_text(&self, text: &str) -> Result<(), ClipboardIoError> {
        let text = text.to_owned();
        tokio::task::spawn_blocking(move || write_system_text(&text))
            .await
            .map_err(|error| ClipboardIoError::Unavailable(error.to_string()))?
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn read_system_text() -> Result<Option<String>, ClipboardIoError> {
    use arboard::Clipboard;

    let mut clipboard =
        Clipboard::new().map_err(|error| ClipboardIoError::Unavailable(error.to_string()))?;

    match clipboard.get_text() {
        Ok(text) => Ok(Some(text)),
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(error) => Err(ClipboardIoError::Unavailable(error.to_string())),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn write_system_text(text: &str) -> Result<(), ClipboardIoError> {
    use arboard::Clipboard;

    let mut clipboard =
        Clipboard::new().map_err(|error| ClipboardIoError::Unavailable(error.to_string()))?;

    clipboard
        .set_text(text.to_owned())
        .map_err(|error| ClipboardIoError::Unavailable(error.to_string()))
}

#[cfg(test)]
pub(crate) mod mock {
    use std::sync::Arc;

    use super::*;
    use tokio::sync::Mutex;

    #[derive(Default)]
    pub struct MockClipboardIo {
        text: Mutex<Option<String>>,
    }

    impl MockClipboardIo {
        pub fn new() -> Arc<Self> {
            Arc::new(Self::default())
        }

        pub async fn current_text(&self) -> Option<String> {
            self.text.lock().await.clone()
        }
    }

    #[async_trait]
    impl ClipboardIo for MockClipboardIo {
        async fn get_text(&self) -> Result<Option<String>, ClipboardIoError> {
            Ok(self.text.lock().await.clone())
        }

        async fn set_text(&self, text: &str) -> Result<(), ClipboardIoError> {
            *self.text.lock().await = Some(text.to_owned());
            Ok(())
        }
    }
}
