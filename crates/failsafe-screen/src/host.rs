use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc as async_mpsc;
use tracing::{debug, warn};
use turbojpeg::{Compressor, Image, PixelFormat};

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
    compressor: Compressor,
}

impl FrameEncoder {
    fn new(settings: &ScreenSettings) -> Result<Self, ScreenHostError> {
        Ok(Self {
            preprocessor: FramePreprocessor::new(settings.max_width),
            compressor: Compressor::new().map_err(|error| ScreenHostError::Encode(error.to_string()))?,
        })
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
        self.preprocessor
            .rgba_to_rgb(frame.rgba, frame.width, frame.height)
            .map_err(ScreenHostError::Encode)?;
        encode_jpeg_rgb(
            &mut self.compressor,
            self.preprocessor.rgb_pixels(),
            width,
            height,
            settings.jpeg_quality,
        )
    }
}

pub async fn run_screen_host(
    mut send: impl AsyncWrite + Unpin + Send + 'static,
    mut recv: impl AsyncRead + Unpin + Send + 'static,
) -> Result<(), ScreenHostError> {
    debug!("screen host started");
    let settings = Arc::new(Mutex::new(ScreenSettings::default()));
    let latest_frame: Arc<Mutex<Option<CapturedFrame>>> = Arc::new(Mutex::new(None));
    let (frame_tx, mut frame_rx) = async_mpsc::channel(1);
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

    let capture_settings = settings.clone();
    let capture_shutdown_rx = shutdown_rx;
    let capture_latest = latest_frame.clone();
    let capture_handle = thread::spawn(move || {
        let mut capturer = match create_capturer() {
            Ok(capturer) => capturer,
            Err(error) => return Err::<(), CaptureError>(error),
        };
        loop {
            if capture_shutdown_rx.try_recv().is_ok() {
                return Ok(());
            }

            let current = *capture_settings.lock().expect("settings lock");
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

            *capture_latest.lock().expect("latest frame lock") = Some(frame);

            let elapsed = started.elapsed();
            if elapsed > interval {
                warn!(
                    frame_ms = elapsed.as_millis() as u64,
                    target_ms = interval.as_millis() as u64,
                    "screen capture over budget"
                );
            } else {
                thread::sleep(interval - elapsed);
            }
        }
    });

    let encode_settings = settings.clone();
    let encode_latest = latest_frame;
    let encode_handle = thread::spawn(move || {
        let initial = *encode_settings.lock().expect("settings lock");
        let mut encoder = match FrameEncoder::new(&initial) {
            Ok(encoder) => encoder,
            Err(error) => {
                warn!("screen encoder init failed: {error}");
                return;
            }
        };

        loop {
            let frame = {
                let mut slot = encode_latest.lock().expect("latest frame lock");
                slot.take()
            };
            let Some(frame) = frame else {
                thread::sleep(Duration::from_millis(1));
                continue;
            };

            let current = *encode_settings.lock().expect("settings lock");
            encoder.sync_settings(&current);

            match encoder.encode_frame(frame, &current) {
                Ok(jpeg) => match frame_tx.try_send(jpeg) {
                    Ok(()) => {}
                    Err(async_mpsc::error::TrySendError::Full(_)) => {
                        warn!("dropping screen frame because viewer is behind");
                    }
                    Err(async_mpsc::error::TrySendError::Closed(_)) => break,
                },
                Err(ScreenHostError::Encode(message)) => {
                    warn!("screen encode failed: {message}");
                }
                Err(_) => unreachable!(),
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
        if send
            .write_all(&packet)
            .await
            .map_err(|error| ScreenHostError::Stream(error.to_string()))
            .is_err()
        {
            break;
        }
    }

    let _ = shutdown_tx.send(());
    control_task.abort();

    match capture_handle.join() {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(error.into()),
        Err(_) => {
            return Err(ScreenHostError::Stream(
                "screen capture thread panicked".to_owned(),
            ));
        }
    }

    if encode_handle.join().is_err() {
        return Err(ScreenHostError::Stream(
            "screen encode thread panicked".to_owned(),
        ));
    }

    Ok(())
}

fn encode_jpeg_rgb(
    compressor: &mut Compressor,
    rgb: &[u8],
    width: u32,
    height: u32,
    quality: u8,
) -> Result<Vec<u8>, ScreenHostError> {
    let width = width as usize;
    let height = height as usize;
    let image = Image {
        pixels: rgb,
        width,
        pitch: width * 3,
        height,
        format: PixelFormat::RGB,
    };
    compressor
        .set_quality(i32::from(quality))
        .map_err(|error| ScreenHostError::Encode(error.to_string()))?;
    compressor
        .compress_to_vec(image)
        .map_err(|error| ScreenHostError::Encode(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turbojpeg_encode_produces_jpeg_magic() {
        let width = 4u32;
        let height = 4u32;
        let rgb = vec![128u8; (width * height * 3) as usize];
        let mut compressor = Compressor::new().expect("compressor");
        let jpeg = encode_jpeg_rgb(&mut compressor, &rgb, width, height, 80).expect("encode");
        assert!(jpeg.len() >= 2);
        assert_eq!(jpeg[0], 0xFF);
        assert_eq!(jpeg[1], 0xD8);
    }
}
