use thiserror::Error;
use xcap::Monitor;

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("no monitors available")]
    NoMonitors,

    #[error("failed to capture screen: {0}")]
    Capture(String),

    #[error("failed to read monitor image: {0}")]
    Image(String),
}

pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn capture_primary_monitor() -> Result<CapturedFrame, CaptureError> {
    let monitors = Monitor::all().map_err(|error| CaptureError::Capture(error.to_string()))?;
    let monitor = monitors
        .into_iter()
        .next()
        .ok_or(CaptureError::NoMonitors)?;

    let image = monitor
        .capture_image()
        .map_err(|error| CaptureError::Image(error.to_string()))?;
    let width = image.width();
    let height = image.height();
    let rgba = image.into_raw();

    Ok(CapturedFrame {
        width,
        height,
        rgba,
    })
}
