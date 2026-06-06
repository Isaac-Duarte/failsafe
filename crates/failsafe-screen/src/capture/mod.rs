#[cfg(target_os = "linux")]
mod linux;

use thiserror::Error;
#[cfg(target_os = "linux")]
use tracing::debug;
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

pub trait ScreenCapturer {
    fn capture(&mut self) -> Result<CapturedFrame, CaptureError>;
}

pub(crate) fn primary_monitor() -> Result<Monitor, CaptureError> {
    let monitors = Monitor::all().map_err(|error| CaptureError::Capture(error.to_string()))?;
    monitors
        .into_iter()
        .next()
        .ok_or(CaptureError::NoMonitors)
}

pub(crate) fn capture_monitor(monitor: &Monitor) -> Result<CapturedFrame, CaptureError> {
    let image = monitor
        .capture_image()
        .map_err(|error| CaptureError::Image(error.to_string()))?;

    Ok(CapturedFrame {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

struct XcapCapturer {
    monitor: Monitor,
}

impl XcapCapturer {
    fn new() -> Result<Self, CaptureError> {
        Ok(Self {
            monitor: primary_monitor()?,
        })
    }
}

impl ScreenCapturer for XcapCapturer {
    fn capture(&mut self) -> Result<CapturedFrame, CaptureError> {
        capture_monitor(&self.monitor)
    }
}

#[cfg(target_os = "linux")]
struct LinuxCapturer(linux::LinuxCapturer);

#[cfg(target_os = "linux")]
impl ScreenCapturer for LinuxCapturer {
    fn capture(&mut self) -> Result<CapturedFrame, CaptureError> {
        self.0.capture()
    }
}

pub fn create_capturer() -> Result<Box<dyn ScreenCapturer>, CaptureError> {
    #[cfg(target_os = "linux")]
    {
        let capturer = linux::LinuxCapturer::new()?;
        debug!("linux screen capture backend initialized");
        Ok(Box::new(LinuxCapturer(capturer)))
    }

    #[cfg(not(target_os = "linux"))]
    Ok(Box::new(XcapCapturer::new()?))
}
