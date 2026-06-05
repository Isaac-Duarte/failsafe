use std::sync::Arc;

use failsafe_core::device::DeviceId;
use failsafe_transport::blobs::{BlobHash, BlobTransfer};

use crate::io::{ClipboardContent, write_received_files};
use crate::limits::ClipboardLimits;
use crate::payload::{ClipboardContent as PayloadContent, ClipboardPayload};

use super::image::decode_image_png;

pub(super) async fn resolve_payload_to_content(
    payload: &ClipboardPayload,
    peer: DeviceId,
    blob_transfer: Arc<dyn BlobTransfer>,
    limits: ClipboardLimits,
) -> Result<ClipboardContent, String> {
    match &payload.content {
        PayloadContent::Text { text } => Ok(ClipboardContent::Text(text.clone())),
        PayloadContent::Html { html, plain } => Ok(ClipboardContent::Html {
            html: html.clone(),
            plain: plain.clone(),
        }),
        PayloadContent::HtmlBlob { hash, plain } => {
            let data = blob_transfer
                .fetch_bytes(peer, &BlobHash::from(hash.as_str()))
                .await
                .map_err(|error| error.to_string())?;
            limits.validate_blob(data.len())?;
            let html = String::from_utf8(data)
                .map_err(|error| format!("clipboard html blob is not valid utf-8: {error}"))?;
            Ok(ClipboardContent::Html {
                html,
                plain: plain.clone(),
            })
        }
        PayloadContent::Image {
            hash,
            width,
            height,
            mime: _,
        } => {
            let data = blob_transfer
                .fetch_bytes(peer, &BlobHash::from(hash.as_str()))
                .await
                .map_err(|error| error.to_string())?;
            limits.validate_blob(data.len())?;
            let mut image = decode_image_png(&data)?;
            image.width = *width;
            image.height = *height;
            Ok(ClipboardContent::Image(image))
        }
        PayloadContent::Files {
            collection_hash,
            entries: _,
        } => {
            let files = blob_transfer
                .fetch_collection_files(peer, &BlobHash::from(collection_hash.as_str()))
                .await
                .map_err(|error| error.to_string())?;
            limits.validate_files(&files)?;
            let paths = write_received_files(&files)
                .await
                .map_err(|error| error.to_string())?;
            Ok(ClipboardContent::Files(paths))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use failsafe_core::device::DeviceId;
    use failsafe_transport::blobs::MockBlobTransfer;

    use super::*;
    use crate::io::ImageDataOwned;
    use crate::limits::ClipboardLimits;
    use crate::payload::ClipboardContent as PayloadContent;

    use super::super::outbound::content_to_payload;

    #[tokio::test]
    async fn image_roundtrip_uses_blob_transfer() {
        let blob_transfer = Arc::new(MockBlobTransfer::new());
        let image = ImageDataOwned {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
        };

        let payload = content_to_payload(
            &ClipboardContent::Image(image.clone()),
            blob_transfer.clone(),
            ClipboardLimits::default(),
        )
        .await
        .unwrap();

        let PayloadContent::Image { ref hash, .. } = payload.content else {
            panic!("expected image payload");
        };

        let peer = DeviceId::new();
        let content = resolve_payload_to_content(
            &payload,
            peer,
            blob_transfer.clone(),
            ClipboardLimits::default(),
        )
        .await
        .unwrap();

        match content {
            ClipboardContent::Image(received) => {
                assert_eq!(received.width, 1);
                assert_eq!(received.height, 1);
                assert!(!hash.is_empty());
            }
            other => panic!("expected image content, got {other:?}"),
        }
    }
}
