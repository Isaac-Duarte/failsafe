use std::path::{Path, PathBuf};

use super::content::{ClipboardContent, ClipboardIoError, ImageDataOwned};

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub fn read_system_clipboard() -> Result<Option<ClipboardContent>, ClipboardIoError> {
    use arboard::Clipboard;

    let mut clipboard =
        Clipboard::new().map_err(|error| ClipboardIoError::Unavailable(error.to_string()))?;

    if let Some(files) = read_clipboard_files(&mut clipboard) {
        return Ok(Some(ClipboardContent::Files(files)));
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
pub fn write_system_clipboard(content: &ClipboardContent) -> Result<(), ClipboardIoError> {
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

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn read_clipboard_files(clipboard: &mut arboard::Clipboard) -> Option<Vec<PathBuf>> {
    if let Ok(files) = clipboard.get().file_list() {
        let existing: Vec<PathBuf> = files.into_iter().filter(|path| path.exists()).collect();
        if !existing.is_empty() {
            return Some(existing);
        }
    }

    #[cfg(target_os = "linux")]
    if let Ok(text) = clipboard.get().text() {
        let paths = super::linux_files::parse_file_paths_from_clipboard_text(&text);
        let existing: Vec<PathBuf> = paths.into_iter().filter(|path| path.exists()).collect();
        if !existing.is_empty() {
            return Some(existing);
        }
    }

    None
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
