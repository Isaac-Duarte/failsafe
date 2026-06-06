use std::thread;
use std::time::{Duration, Instant};

use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder};
use thiserror::Error;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::capture::{CaptureError, CapturedFrame, capture_primary_monitor};
use crate::preprocess::FramePreprocessor;
use crate::protocol::encode_frame;

const TARGET_FPS: u64 = 15;
const JPEG_QUALITY: u8 = 60;
const MAX_FRAME_WIDTH: u32 = 1280;

#[derive(Debug, Error)]
pub enum ScreenHostError {
    #[error("capture error: {0}")]
    Capture(#[from] CaptureError),

    #[error("encode error: {0}")]
    Encode(String),

    #[error("stream error: {0}")]
    Stream(String),
}

struct FrameEncoder {
    preprocessor: FramePreprocessor,
}

impl FrameEncoder {
    fn new() -> Self {
        Self {
            preprocessor: FramePreprocessor::new(MAX_FRAME_WIDTH),
        }
    }

    fn encode_frame(&mut self, frame: CapturedFrame) -> Result<Vec<u8>, ScreenHostError> {
        let (width, height) = crate::preprocess::output_dimensions(
            frame.width,
            frame.height,
            MAX_FRAME_WIDTH,
        );
        let rgb = self
            .preprocessor
            .rgba_to_rgb(frame.rgba, frame.width, frame.height)
            .map_err(ScreenHostError::Encode)?;
        encode_jpeg_rgb(&rgb, width, height)
    }
}

pub async fn run_screen_host(mut send: impl AsyncWrite + Unpin) -> Result<(), ScreenHostError> {
    debug!("screen host started");
    let interval = Duration::from_millis(1000 / TARGET_FPS);
    let (frame_tx, mut frame_rx) = mpsc::channel(1);

    let capture_handle = thread::spawn(move || {
        let mut encoder = FrameEncoder::new();
        loop {
            let started = Instant::now();
            let frame = match capture_primary_monitor() {
                Ok(frame) => frame,
                Err(error) => return Err::<(), CaptureError>(error),
            };

            match encoder.encode_frame(frame) {
                Ok(jpeg) => {
                    if frame_tx.blocking_send(jpeg).is_err() {
                        return Ok(());
                    }
                }
                Err(ScreenHostError::Encode(message)) => {
                    warn!("screen encode failed: {message}");
                }
                Err(_) => unreachable!(),
            }

            let elapsed = started.elapsed();
            if elapsed > interval {
                warn!(
                    frame_ms = elapsed.as_millis() as u64,
                    target_ms = interval.as_millis() as u64,
                    "screen frame over budget"
                );
            } else {
                thread::sleep(interval - elapsed);
            }
        }
    });

    while let Some(jpeg) = frame_rx.recv().await {
        let packet = encode_frame(&jpeg);
        send.write_all(&packet)
            .await
            .map_err(|error| ScreenHostError::Stream(error.to_string()))?;
    }

    match capture_handle.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.into()),
        Err(_) => Err(ScreenHostError::Stream(
            "screen capture thread panicked".to_owned(),
        )),
    }
}

fn encode_jpeg_rgb(rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>, ScreenHostError> {
    let mut jpeg = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY);
    encoder
        .write_image(rgb, width, height, ExtendedColorType::Rgb8)
        .map_err(|error| ScreenHostError::Encode(error.to_string()))?;
    Ok(jpeg)
}
