use std::sync::Arc;

use failsafe_transport::blobs::BlobTransfer;

use crate::io::ClipboardContent;
use crate::limits::ClipboardLimits;
use crate::payload::{
    self, ClipboardContent as PayloadContent, ClipboardPayload, FileEntry, INLINE_HTML_THRESHOLD,
};

use super::image::encode_image_png;

pub(super) async fn content_to_payload(
    content: &ClipboardContent,
    blob_transfer: Arc<dyn BlobTransfer>,
    limits: ClipboardLimits,
) -> Result<ClipboardPayload, String> {
    let content = match content {
        ClipboardContent::Text(text) => PayloadContent::Text { text: text.clone() },
        ClipboardContent::Html { html, plain } => {
            if html.len() <= INLINE_HTML_THRESHOLD {
                PayloadContent::Html {
                    html: html.clone(),
                    plain: plain.clone(),
                }
            } else {
                limits.validate_blob(html.len())?;
                let hash = blob_transfer
                    .store_bytes(html.as_bytes().to_vec())
                    .await
                    .map_err(|error| error.to_string())?;
                PayloadContent::HtmlBlob {
                    hash: hash.as_str().to_owned(),
                    plain: plain.clone(),
                }
            }
        }
        ClipboardContent::Image(image) => {
            let png = encode_image_png(image)?;
            limits.validate_blob(png.len())?;
            let hash = blob_transfer
                .store_bytes(png)
                .await
                .map_err(|error| error.to_string())?;
            PayloadContent::Image {
                hash: hash.as_str().to_owned(),
                width: image.width,
                height: image.height,
                mime: "image/png".to_owned(),
            }
        }
        ClipboardContent::Files(paths) => {
            let mut files = Vec::with_capacity(paths.len());
            for path in paths {
                if !path.exists() {
                    continue;
                }
                let data = tokio::fs::read(path)
                    .await
                    .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
                let name = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("file")
                    .to_owned();
                files.push((name, data));
            }
            if files.is_empty() {
                return Err("clipboard file paths are missing or unreadable".to_owned());
            }
            limits.validate_files(&files)?;
            let hash = blob_transfer
                .store_files(files.clone())
                .await
                .map_err(|error| error.to_string())?;
            let entries = files
                .into_iter()
                .map(|(name, data)| FileEntry {
                    name,
                    size: data.len() as u64,
                })
                .collect();
            PayloadContent::Files {
                collection_hash: hash.as_str().to_owned(),
                entries,
            }
        }
    };

    Ok(ClipboardPayload {
        version: payload::CLIPBOARD_PAYLOAD_VERSION,
        content,
    })
}

pub(super) fn fingerprint_content(content: &ClipboardContent) -> String {
    let seed = match content {
        ClipboardContent::Text(text) => format!("text:{text}"),
        ClipboardContent::Html { html, plain } => format!("html:{html}\0{plain}"),
        ClipboardContent::Image(image) => {
            format!(
                "image:{}x{}:{}",
                image.width,
                image.height,
                hex::encode(blake3::hash(&image.rgba).as_bytes())
            )
        }
        ClipboardContent::Files(paths) => {
            let joined = paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join("\0");
            format!("files:{joined}")
        }
    };
    hex::encode(blake3::hash(seed.as_bytes()).as_bytes())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use failsafe_transport::blobs::MockBlobTransfer;

    use super::*;
    use crate::limits::ClipboardLimits;

    #[tokio::test]
    async fn missing_file_paths_fail_payload_build() {
        let result = content_to_payload(
            &ClipboardContent::Files(vec![PathBuf::from("/no/such/file.csv")]),
            Arc::new(MockBlobTransfer::new()),
            ClipboardLimits::default(),
        )
        .await;

        assert!(result.is_err());
    }
}
