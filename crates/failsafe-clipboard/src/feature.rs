use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use failsafe_core::device::DeviceId;
use failsafe_core::feature::{Feature, FeatureError, FeatureId};
use failsafe_core::message::FeatureMessage;
use failsafe_core::outbound::{OutboundMessage, OutboundPublisher};
use failsafe_transport::blobs::{BlobHash, BlobTransfer, MockBlobTransfer};
use image::{DynamicImage, ImageBuffer, RgbaImage};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::io::{
    ClipboardContent, ClipboardIo, ClipboardIoError, ImageDataOwned, SystemClipboardIo,
    write_received_files,
};
use crate::limits::ClipboardLimits;
use crate::payload::{
    self, ClipboardContent as PayloadContent, ClipboardPayload, FileEntry, INLINE_HTML_THRESHOLD,
};

const POLL_INTERVAL: Duration = Duration::from_millis(300);

struct ClipboardState {
    publisher: Arc<dyn OutboundPublisher>,
    clipboard: Arc<dyn ClipboardIo>,
    blob_transfer: Arc<dyn BlobTransfer>,
    limits: ClipboardLimits,
    last_emitted: Mutex<Option<String>>,
    last_failed: Mutex<Option<String>>,
    applying_remote: AtomicBool,
}

/// Syncs clipboard content with peer devices via the runtime publisher.
pub struct ClipboardFeature {
    state: Arc<ClipboardState>,
    watch_task: Option<JoinHandle<()>>,
}

impl ClipboardFeature {
    pub fn new(
        publisher: Arc<dyn OutboundPublisher>,
        blob_transfer: Option<Arc<dyn BlobTransfer>>,
    ) -> Self {
        Self::new_with_limits(publisher, blob_transfer, ClipboardLimits::default())
    }

    pub fn new_with_limits(
        publisher: Arc<dyn OutboundPublisher>,
        blob_transfer: Option<Arc<dyn BlobTransfer>>,
        limits: ClipboardLimits,
    ) -> Self {
        Self::with_dependencies(
            publisher,
            Arc::new(SystemClipboardIo),
            blob_transfer.unwrap_or_else(|| Arc::new(MockBlobTransfer::new())),
            limits,
        )
    }

    fn with_dependencies(
        publisher: Arc<dyn OutboundPublisher>,
        clipboard: Arc<dyn ClipboardIo>,
        blob_transfer: Arc<dyn BlobTransfer>,
        limits: ClipboardLimits,
    ) -> Self {
        Self {
            state: Arc::new(ClipboardState {
                publisher,
                clipboard,
                blob_transfer,
                limits,
                last_emitted: Mutex::new(None),
                last_failed: Mutex::new(None),
                applying_remote: AtomicBool::new(false),
            }),
            watch_task: None,
        }
    }
}

#[async_trait]
impl Feature for ClipboardFeature {
    fn id(&self) -> FeatureId {
        FeatureId::Clipboard
    }

    async fn start(&mut self) -> Result<(), FeatureError> {
        if self.watch_task.is_some() {
            return Ok(());
        }

        let state = self.state.clone();
        self.watch_task = Some(tokio::spawn(async move {
            watch_clipboard(state).await;
        }));

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), FeatureError> {
        if let Some(task) = self.watch_task.take() {
            task.abort();
        }
        Ok(())
    }

    async fn handle_message(&mut self, message: FeatureMessage) -> Result<(), FeatureError> {
        let payload = payload::decode(&message.payload)?;
        let content = resolve_payload_to_content(
            &payload,
            message.from,
            self.state.blob_transfer.clone(),
            self.state.limits,
        )
        .await
        .map_err(|error| FeatureError::Failed(FeatureId::Clipboard, error))?;

        self.state.applying_remote.store(true, Ordering::SeqCst);

        let result = self
            .state
            .clipboard
            .write(&content)
            .await
            .map_err(io_error_to_feature_error);

        *self.state.last_emitted.lock().await = Some(payload::fingerprint(&payload));
        self.state.applying_remote.store(false, Ordering::SeqCst);

        result
    }
}

async fn watch_clipboard(state: Arc<ClipboardState>) {
    let mut interval = tokio::time::interval(POLL_INTERVAL);

    loop {
        interval.tick().await;

        if state.applying_remote.load(Ordering::SeqCst) {
            continue;
        }

        let content = match state.clipboard.read().await {
            Ok(Some(content)) => content,
            Ok(None) => continue,
            Err(error) => {
                eprintln!("clipboard read failed: {error}");
                continue;
            }
        };

        let content_fingerprint = fingerprint_content(&content);
        let payload =
            match content_to_payload(&content, state.blob_transfer.clone(), state.limits).await {
                Ok(payload) => {
                    *state.last_failed.lock().await = None;
                    payload
                }
                Err(error) => {
                    let mut last_failed = state.last_failed.lock().await;
                    if last_failed.as_deref() == Some(content_fingerprint.as_str()) {
                        continue;
                    }
                    *last_failed = Some(content_fingerprint);
                    eprintln!("clipboard payload build failed: {error}");
                    continue;
                }
            };

        let fingerprint = payload::fingerprint(&payload);
        {
            let last_emitted = state.last_emitted.lock().await;
            if last_emitted.as_deref() == Some(fingerprint.as_str()) {
                continue;
            }
        }

        *state.last_emitted.lock().await = Some(fingerprint);

        let outbound = OutboundMessage::new(FeatureId::Clipboard, payload::encode(&payload));

        if let Err(error) = state.publisher.publish(outbound).await {
            eprintln!("clipboard publish failed: {error}");
        }
    }
}

async fn content_to_payload(
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

async fn resolve_payload_to_content(
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

fn fingerprint_content(content: &ClipboardContent) -> String {
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

fn encode_image_png(image: &ImageDataOwned) -> Result<Vec<u8>, String> {
    let buffer: RgbaImage = ImageBuffer::from_raw(image.width, image.height, image.rgba.clone())
        .ok_or_else(|| "invalid clipboard image dimensions".to_owned())?;
    let mut encoded = Vec::new();
    DynamicImage::ImageRgba8(buffer)
        .write_to(
            &mut std::io::Cursor::new(&mut encoded),
            image::ImageFormat::Png,
        )
        .map_err(|error| error.to_string())?;
    Ok(encoded)
}

fn decode_image_png(data: &[u8]) -> Result<ImageDataOwned, String> {
    let image = image::load_from_memory(data).map_err(|error| error.to_string())?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok(ImageDataOwned {
        width,
        height,
        rgba: rgba.into_raw(),
    })
}

fn io_error_to_feature_error(error: ClipboardIoError) -> FeatureError {
    FeatureError::Failed(FeatureId::Clipboard, error.to_string())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use failsafe_core::device::DeviceId;
    use failsafe_core::feature::Feature;
    use failsafe_core::peer::PeerDirectory;
    use failsafe_transport::mock::MockTransport;
    use failsafe_transport::router::MessageRouter;
    use failsafe_transport::transport::Transport;

    use super::*;
    use crate::io::mock::MockClipboardIo;
    use crate::payload::{ClipboardContent as PayloadContent, ClipboardPayload};

    #[tokio::test]
    async fn handle_message_updates_clipboard() {
        let (transport, _peer) = MockTransport::pair().await;
        let publisher =
            MessageRouter::into_publisher(Arc::new(transport), Arc::new(PeerDirectory::new()));
        let clipboard = MockClipboardIo::new();

        let mut feature = ClipboardFeature::with_dependencies(
            publisher,
            clipboard.clone(),
            Arc::new(MockBlobTransfer::new()),
            ClipboardLimits::default(),
        );

        let payload = ClipboardPayload {
            version: payload::CLIPBOARD_PAYLOAD_VERSION,
            content: PayloadContent::Text {
                text: "synced".to_owned(),
            },
        };

        feature
            .handle_message(FeatureMessage::new(
                DeviceId::new(),
                DeviceId::new(),
                FeatureId::Clipboard,
                payload::encode(&payload),
            ))
            .await
            .unwrap();

        assert_eq!(
            clipboard.current().await,
            Some(ClipboardContent::Text("synced".to_owned()))
        );
    }

    #[tokio::test]
    async fn watch_broadcasts_local_changes() {
        let (local_transport, peer_transport) = MockTransport::pair().await;
        let peer_id = peer_transport.local_device_id();

        let peers = Arc::new(PeerDirectory::new());
        peers.replace_peers([peer_id]).await;

        let publisher = MessageRouter::into_publisher(Arc::new(local_transport), peers);

        let clipboard = MockClipboardIo::new();
        clipboard
            .write(&ClipboardContent::Text("local copy".to_owned()))
            .await
            .unwrap();

        let mut feature = ClipboardFeature::with_dependencies(
            publisher,
            clipboard,
            Arc::new(MockBlobTransfer::new()),
            ClipboardLimits::default(),
        );
        feature.start().await.unwrap();

        tokio::time::sleep(Duration::from_millis(400)).await;

        let received = peer_transport.recv().await.unwrap();
        let payload = payload::decode(&received.payload).unwrap();
        assert_eq!(
            payload.content,
            PayloadContent::Text {
                text: "local copy".to_owned()
            }
        );

        feature.stop().await.unwrap();
    }

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
