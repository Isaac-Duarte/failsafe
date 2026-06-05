use std::path::PathBuf;

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
    Html { html: String, plain: String },
    Image(ImageDataOwned),
    Files(Vec<PathBuf>),
}

#[derive(Debug, Error)]
pub enum ClipboardIoError {
    #[error("clipboard unavailable: {0}")]
    Unavailable(String),
}
