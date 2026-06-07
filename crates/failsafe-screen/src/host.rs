use iroh::endpoint::SendStream;

use crate::monitor::ScreenError;

#[cfg(feature = "scap-capture")]
use crate::protocol::write_nal;

pub async fn write_screen_list(mut send: SendStream) -> Result<(), ScreenError> {
    let screens = crate::monitor::list_displays()?;
    let json = serde_json::to_vec(&screens)
        .map_err(|error| ScreenError::Capture(error.to_string()))?;
    let len = u32::try_from(json.len())
        .map_err(|_| ScreenError::Capture("screen list too large".to_owned()))?;
    send.write_all(&len.to_be_bytes())
        .await
        .map_err(|error| ScreenError::Transport(error.to_string()))?;
    send.write_all(&json)
        .await
        .map_err(|error| ScreenError::Transport(error.to_string()))?;
    send.finish()
        .map_err(|error| ScreenError::Transport(error.to_string()))?;
    Ok(())
}

#[cfg(not(feature = "scap-capture"))]
pub async fn run_screen_host(_screen_id: u32, _send: SendStream) -> Result<(), ScreenError> {
    Err(ScreenError::CaptureUnavailable)
}

#[cfg(feature = "scap-capture")]
mod capture {
    use std::time::{Duration, Instant};

    use scap::capturer::{Capturer, Options, Resolution};
    use scap::frame::{Frame, FrameType, VideoFrame};

    use super::*;
    use crate::encode::H264Encoder;
    use crate::monitor::ScreenError;

    const TARGET_FPS: u32 = 15;

    pub async fn run_screen_host(screen_id: u32, mut send: SendStream) -> Result<(), ScreenError> {
        let (async_tx, mut async_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(8);
        let capture_handle = std::thread::spawn(move || {
            if let Err(error) = capture_loop(screen_id, async_tx) {
                tracing::warn!("screen capture ended: {error}");
            }
        });

        while let Some(nal) = async_rx.recv().await {
            write_nal(&mut send, &nal)
                .await
                .map_err(|error| ScreenError::Transport(error.to_string()))?;
        }

        let _ = send.finish();
        let _ = capture_handle.join();
        Ok(())
    }

    fn resolve_display_target(screen_id: u32) -> Result<Option<scap::Target>, ScreenError> {
        crate::monitor::ensure_capture_platform()?;

        #[cfg(target_os = "linux")]
        {
            let _ = screen_id;
            return Ok(None);
        }

        #[cfg(target_os = "macos")]
        {
            let targets = scap::get_all_targets();
            let mut index = 0u32;
            for target in targets {
                let scap::Target::Display(display) = target else {
                    continue;
                };
                if index == screen_id {
                    return Ok(Some(scap::Target::Display(display)));
                }
                index += 1;
            }
            Err(ScreenError::Capture(format!(
                "display {screen_id} is not available"
            )))
        }
    }

    fn capture_loop(
        screen_id: u32,
        tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    ) -> Result<(), ScreenError> {
        let target = resolve_display_target(screen_id)?;
        let options = Options {
            fps: TARGET_FPS,
            target,
            show_cursor: true,
            show_highlight: false,
            output_type: FrameType::BGRAFrame,
            output_resolution: Resolution::_720p,
            ..Default::default()
        };

        let mut capturer =
            Capturer::build(options).map_err(|error| ScreenError::Capture(error.to_string()))?;
        capturer.start_capture();

        let mut encoder: Option<H264Encoder> = None;
        let frame_interval = Duration::from_secs_f64(1.0 / f64::from(TARGET_FPS));

        loop {
            let started = Instant::now();
            let frame = match capturer.get_next_frame() {
                Ok(frame) => frame,
                Err(_) => break,
            };

            let Frame::Video(video) = frame else {
                continue;
            };

            let Some((width, height, bgra)) = extract_bgra(video) else {
                continue;
            };

            let enc = match &mut encoder {
                Some(enc) => enc,
                None => {
                    encoder = Some(H264Encoder::new(width, height)?);
                    encoder.as_mut().expect("encoder just set")
                }
            };

            let nal = enc.encode_bgra(width, height, &bgra)?;
            if tx.blocking_send(nal).is_err() {
                break;
            }

            let elapsed = started.elapsed();
            if elapsed < frame_interval {
                std::thread::sleep(frame_interval - elapsed);
            }
        }

        capturer.stop_capture();
        Ok(())
    }

    fn extract_bgra(frame: VideoFrame) -> Option<(u32, u32, Vec<u8>)> {
        match frame {
            VideoFrame::BGRA(data) => {
                let width = data.width.max(0) as u32;
                let height = data.height.max(0) as u32;
                if width == 0 || height == 0 {
                    return None;
                }
                Some((width, height, data.data))
            }
            _ => None,
        }
    }
}

#[cfg(feature = "scap-capture")]
pub use capture::run_screen_host;
