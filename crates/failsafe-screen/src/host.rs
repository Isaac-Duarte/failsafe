use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::capture::{CaptureError, CapturedFrame, create_capturer};
use crate::preprocess::FramePreprocessor;
use crate::protocol::{
    PACKET_TAG_CONTROL, ProtocolError, decode_control, encode_frame, read_tagged_packet,
};
use crate::quality::ScreenSettings;

#[derive(Debug, Error)]
pub enum ScreenHostError {
    #[error("capture error: {0}")]
    Capture(#[from] CaptureError),

    #[error("encode error: {0}")]
    Encode(String),

    #[error("stream error: {0}")]
    Stream(String),

    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),
}

struct FrameEncoder {
    preprocessor: FramePreprocessor,
}

impl FrameEncoder {
    fn new(settings: &ScreenSettings) -> Self {
        Self {
            preprocessor: FramePreprocessor::new(settings.max_width),
        }
    }

    fn sync_settings(&mut self, settings: &ScreenSettings) {
        self.preprocessor.set_max_width(settings.max_width);
    }

    fn encode_frame(
        &mut self,
        frame: CapturedFrame,
        settings: &ScreenSettings,
    ) -> Result<Vec<u8>, ScreenHostError> {
        let (width, height) =
            crate::preprocess::output_dimensions(frame.width, frame.height, settings.max_width);
        let rgb = self
            .preprocessor
            .rgba_to_rgb(frame.rgba, frame.width, frame.height)
            .map_err(ScreenHostError::Encode)?;
        encode_jpeg_rgb(&rgb, width, height, settings.jpeg_quality)
    }
}

pub async fn run_screen_host(
    mut send: impl AsyncWrite + Unpin + Send + 'static,
    mut recv: impl AsyncRead + Unpin + Send + 'static,
) -> Result<(), ScreenHostError> {
    debug!("screen host started");
    let settings = Arc::new(Mutex::new(ScreenSettings::default()));
    let (frame_tx, mut frame_rx) = mpsc::channel(1);
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel::<()>();

    let capture_settings = settings.clone();
    let capture_shutdown_rx = shutdown_rx;
    let capture_handle = thread::spawn(move || {
        let initial = *capture_settings.lock().expect("settings lock");
        let mut capturer = match create_capturer() {
            Ok(capturer) => capturer,
            Err(error) => return Err::<(), CaptureError>(error),
        };
        let mut encoder = FrameEncoder::new(&initial);
        loop {
            if capture_shutdown_rx.try_recv().is_ok() {
                return Ok(());
            }

            let current = *capture_settings.lock().expect("settings lock");
            encoder.sync_settings(&current);
            let interval = Duration::from_millis(1000 / current.target_fps.max(1));

            let started = Instant::now();
            let frame = match capturer.capture() {
                Ok(frame) => frame,
                Err(error) => {
                    warn!("screen capture failed: {error}");
                    thread::sleep(Duration::from_millis(250));
                    continue;
                }
            };

            match encoder.encode_frame(frame, &current) {
                Ok(jpeg) => match frame_tx.try_send(jpeg) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        warn!("dropping screen frame because viewer is behind");
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => return Ok(()),
                },
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

    let control_settings = settings.clone();
    let control_task = tokio::spawn(async move {
        loop {
            match read_tagged_packet(&mut recv).await {
                Ok((PACKET_TAG_CONTROL, payload)) => match decode_control(&payload) {
                    Ok(message) => {
                        let mut current = control_settings.lock().expect("settings lock");
                        message.apply(&mut current);
                        debug!(
                            max_width = current.max_width,
                            jpeg_quality = current.jpeg_quality,
                            target_fps = current.target_fps,
                            "updated screen quality settings"
                        );
                    }
                    Err(error) => warn!("invalid screen control message: {error}"),
                },
                Ok((tag, _)) => warn!("unexpected screen packet tag from viewer: {tag}"),
                Err(ProtocolError::Io(error))
                    if error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(error) => {
                    warn!("screen control reader failed: {error}");
                    break;
                }
            }
        }
    });

    while let Some(jpeg) = frame_rx.recv().await {
        let packet = encode_frame(&jpeg);
        send.write_all(&packet)
            .await
            .map_err(|error| ScreenHostError::Stream(error.to_string()))?;
    }

    let _ = shutdown_tx.send(());
    control_task.abort();

    match capture_handle.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.into()),
        Err(_) => Err(ScreenHostError::Stream(
            "screen capture thread panicked".to_owned(),
        )),
    }
}

fn encode_jpeg_rgb(
    rgb: &[u8],
    width: u32,
    height: u32,
    quality: u8,
) -> Result<Vec<u8>, ScreenHostError> {
    let mut jpeg = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut jpeg, quality);
    encoder
        .write_image(rgb, width, height, ExtendedColorType::Rgb8)
        .map_err(|error| ScreenHostError::Encode(error.to_string()))?;
    Ok(jpeg)
}
