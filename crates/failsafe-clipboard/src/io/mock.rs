use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use super::{ClipboardContent, ClipboardIo, ClipboardIoError};

#[derive(Default)]
pub struct MockClipboardIo {
    content: Mutex<Option<ClipboardContent>>,
}

impl MockClipboardIo {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn current(&self) -> Option<ClipboardContent> {
        self.content.lock().await.clone()
    }
}

#[async_trait]
impl ClipboardIo for MockClipboardIo {
    async fn read(&self) -> Result<Option<ClipboardContent>, ClipboardIoError> {
        Ok(self.content.lock().await.clone())
    }

    async fn write(&self, content: &ClipboardContent) -> Result<(), ClipboardIoError> {
        *self.content.lock().await = Some(content.clone());
        Ok(())
    }
}
