use std::time::Duration;

use failsafe_transport::iroh::DesktopSession;
use iroh::endpoint::{RecvStream, SendStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::interval;
use tracing::{debug, warn};

use crate::capture::{self, CaptureError};
use crate::protocol::{encode_config, encode_jpeg, encode_length_prefixed};

const TARGET_FPS: u64 = 15;
const JPEG_QUALITY: u8 = 75;

pub async fn run_desktop_host(session: DesktopSession) {
    let device = session.from;
    debug!(%device, display = session.display_index, view_only = session.view_only, "starting desktop host");

    let video = tokio::spawn(run_video_host(
        session.send,
        session.recv,
        session.display_index,
    ));

    let _ = video.await;
    debug!(%device, "desktop host finished");
}

async fn run_video_host(mut send: SendStream, mut recv: RecvStream, display_index: u32) {
    let mut ticker = interval(Duration::from_millis(1000 / TARGET_FPS));
    let mut remote_width = 0u32;
    let mut remote_height = 0u32;
    let mut drain_buf = [0u8; 64];

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match capture::capture_display_jpeg(display_index, JPEG_QUALITY) {
                    Ok((jpeg, width, height)) => {
                        if remote_width != width || remote_height != height {
                            remote_width = width;
                            remote_height = height;
                            if let Err(error) = write_frame(&mut send, &encode_config(width, height)).await {
                                warn!("failed to send config frame: {error}");
                                break;
                            }
                        }
                        let payload = encode_jpeg(&jpeg);
                        if let Err(error) = write_frame(&mut send, &payload).await {
                            warn!("failed to send jpeg frame: {error}");
                            break;
                        }
                    }
                    Err(CaptureError::Platform(error)) => {
                        warn!("capture failed: {error}");
                        ticker.reset();
                    }
                    Err(CaptureError::Encode(error)) => {
                        warn!("encode failed: {error}");
                    }
                }
            }
            read = recv.read(&mut drain_buf) => {
                match read {
                    Ok(Some(0)) | Ok(None) => break,
                    Ok(Some(_)) => {}
                    Err(error) => {
                        debug!("desktop video stream closed: {error}");
                        break;
                    }
                }
            }
        }
    }

    let _ = send.finish();
}

async fn write_frame(send: &mut SendStream, payload: &[u8]) -> Result<(), String> {
    let frame = encode_length_prefixed(payload);
    send.write_all(&frame)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}
