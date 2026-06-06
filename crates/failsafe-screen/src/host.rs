use std::time::Duration;

use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder};
use thiserror::Error;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

use crate::capture::{CaptureError, capture_primary_monitor};
use crate::protocol::encode_frame;

const TARGET_FPS: u64 = 12;
const JPEG_QUALITY: u8 = 70;

#[derive(Debug, Error)]
pub enum ScreenHostError {
    #[error("capture error: {0}")]
    Capture(#[from] CaptureError),

    #[error("encode error: {0}")]
    Encode(String),

    #[error("stream error: {0}")]
    Stream(String),
}

pub async fn run_screen_host(mut send: impl AsyncWrite + Unpin) -> Result<(), ScreenHostError> {
    debug!("screen host started");
    let interval = Duration::from_millis(1000 / TARGET_FPS);

    loop {
        let started = std::time::Instant::now();
        match capture_primary_monitor() {
            Ok(frame) => {
                let jpeg = encode_jpeg(&frame.rgba, frame.width, frame.height)?;
                let packet = encode_frame(&jpeg);
                send.write_all(&packet)
                    .await
                    .map_err(|error| ScreenHostError::Stream(error.to_string()))?;
            }
            Err(error) => {
                warn!("screen capture failed: {error}");
                return Err(error.into());
            }
        }

        let elapsed = started.elapsed();
        if elapsed < interval {
            tokio::time::sleep(interval - elapsed).await;
        }
    }
}

fn encode_jpeg(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, ScreenHostError> {
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for pixel in rgba.chunks_exact(4) {
        rgb.extend_from_slice(&pixel[..3]);
    }

    let mut jpeg = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY);
    encoder
        .write_image(&rgb, width, height, ExtendedColorType::Rgb8)
        .map_err(|error| ScreenHostError::Encode(error.to_string()))?;
    Ok(jpeg)
}
