use std::path::{Path, PathBuf};

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageDataOwned {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardContent {
    Text(String),
    Html {
        html: String,
        plain: String,
    },
    Image(ImageDataOwned),
    Files(Vec<PathBuf>),
}

#[derive(Debug, Error)]
pub enum ClipboardIoError {
    #[error("clipboard unavailable: {0}")]
    Unavailable(String),
}

#[async_trait]
pub trait ClipboardIo: Send + Sync {
    async fn read(&self) -> Result<Option<ClipboardContent>, ClipboardIoError>;

    async fn write(&self, content: &ClipboardContent) -> Result<(), ClipboardIoError>;
}

pub struct SystemClipboardIo;

#[async_trait]
impl ClipboardIo for SystemClipboardIo {
    async fn read(&self) -> Result<Option<ClipboardContent>, ClipboardIoError> {
        tokio::task::spawn_blocking(read_system_clipboard)
            .await
            .map_err(|error| ClipboardIoError::Unavailable(error.to_string()))?
    }

    async fn write(&self, content: &ClipboardContent) -> Result<(), ClipboardIoError> {
        let content = content.clone();
        tokio::task::spawn_blocking(move || write_system_clipboard(&content))
            .await
            .map_err(|error| ClipboardIoError::Unavailable(error.to_string()))?
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn read_system_clipboard() -> Result<Option<ClipboardContent>, ClipboardIoError> {
    use arboard::Clipboard;

    let mut clipboard =
        Clipboard::new().map_err(|error| ClipboardIoError::Unavailable(error.to_string()))?;

    if let Ok(files) = clipboard.get().file_list() {
        if !files.is_empty() {
            return Ok(Some(ClipboardContent::Files(files)));
        }
    }

    if let Ok(image) = clipboard.get().image() {
        return Ok(Some(ClipboardContent::Image(ImageDataOwned {
            width: image.width as u32,
            height: image.height as u32,
            rgba: image.bytes.into_owned(),
        })));
    }

    if let Ok(html) = clipboard.get().html() {
        let plain = clipboard
            .get()
            .text()
            .unwrap_or_else(|_| strip_html_tags(&html));
        return Ok(Some(ClipboardContent::Html { html, plain }));
    }

    match clipboard.get().text() {
        Ok(text) => Ok(Some(ClipboardContent::Text(text))),
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(error) => Err(ClipboardIoError::Unavailable(error.to_string())),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn write_system_clipboard(content: &ClipboardContent) -> Result<(), ClipboardIoError> {
    use arboard::{Clipboard, ImageData};

    let mut clipboard =
        Clipboard::new().map_err(|error| ClipboardIoError::Unavailable(error.to_string()))?;

    match content {
        ClipboardContent::Text(text) => clipboard
            .set()
            .text(text)
            .map_err(|error| ClipboardIoError::Unavailable(error.to_string())),
        ClipboardContent::Html { html, plain } => clipboard
            .set()
            .html(html, Some(plain))
            .map_err(|error| ClipboardIoError::Unavailable(error.to_string())),
        ClipboardContent::Image(image) => {
            let image_data = ImageData {
                width: image.width as usize,
                height: image.height as usize,
                bytes: std::borrow::Cow::Borrowed(&image.rgba),
            };
            clipboard
                .set()
                .image(image_data)
                .map_err(|error| ClipboardIoError::Unavailable(error.to_string()))
        }
        ClipboardContent::Files(paths) => {
            let refs: Vec<&Path> = paths.iter().map(|path| path.as_path()).collect();
            clipboard
                .set()
                .file_list(&refs)
                .map_err(|error| ClipboardIoError::Unavailable(error.to_string()))
        }
    }
}

fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

pub fn default_clipboard_cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("failsafe").join("clipboard"))
}

pub async fn write_received_files(
    files: &[(String, Vec<u8>)],
) -> Result<Vec<PathBuf>, ClipboardIoError> {
    let base = default_clipboard_cache_dir()
        .ok_or_else(|| ClipboardIoError::Unavailable("clipboard cache dir unavailable".to_owned()))?;
    let session = base.join(uuid::Uuid::new_v4().to_string());
    tokio::fs::create_dir_all(&session)
        .await
        .map_err(|error| ClipboardIoError::Unavailable(error.to_string()))?;

    let mut paths = Vec::with_capacity(files.len());
    for (name, data) in files {
        let path = session.join(sanitize_filename(name));
        tokio::fs::write(&path, data)
            .await
            .map_err(|error| ClipboardIoError::Unavailable(error.to_string()))?;
        paths.push(path);
    }

    Ok(paths)
}

fn sanitize_filename(name: &str) -> String {
    let candidate = Path::new(name)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    if candidate.is_empty() {
        "file".to_owned()
    } else {
        candidate.to_owned()
    }
}

#[cfg(test)]
pub(crate) mod mock {
    use std::sync::Arc;

    use super::*;
    use tokio::sync::Mutex;

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
}
