use libwayshot_xcap::region::LogicalRegion;
use libwayshot_xcap::WayshotConnection;
use scrap::{Capturer, Display};
use std::io::ErrorKind;
use tracing::{debug, warn};
use xcap::Monitor;

use super::{CaptureError, CapturedFrame};

pub struct LinuxCapturer {
    backend: LinuxBackend,
}

enum LinuxBackend {
    X11 {
        capturer: Capturer,
        width: u32,
        height: u32,
    },
    WaylandWlroots {
        connection: WayshotConnection,
        region: LogicalRegion,
        width: u32,
        height: u32,
    },
    Xcap,
}

impl LinuxCapturer {
    pub fn new() -> Result<Self, CaptureError> {
        if is_wayland_session() {
            if is_wlroots_desktop() {
                if let Ok(capturer) = Self::try_wayland_wlroots() {
                    debug!("using wayland wlroots screen capture");
                    return Ok(capturer);
                }
                warn!("wlroots screen capture unavailable, falling back to xcap");
            } else {
                debug!("non-wlroots wayland session, using xcap screen capture");
            }
            Ok(Self {
                backend: LinuxBackend::Xcap,
            })
        } else if let Ok(capturer) = Self::try_x11() {
            debug!("using x11 scrap screen capture");
            Ok(capturer)
        } else {
            debug!("falling back to xcap screen capture on x11");
            Ok(Self {
                backend: LinuxBackend::Xcap,
            })
        }
    }

    pub fn capture(&mut self) -> Result<CapturedFrame, CaptureError> {
        match &mut self.backend {
            LinuxBackend::X11 {
                capturer,
                width,
                height,
            } => capture_x11(capturer, *width, *height),
            LinuxBackend::WaylandWlroots {
                connection,
                region,
                width,
                height,
            } => match capture_wayland_wlroots(connection, region, *width, *height) {
                Ok(frame) => Ok(frame),
                Err(error) => {
                    warn!("wlroots capture failed, falling back to xcap: {error}");
                    self.backend = LinuxBackend::Xcap;
                    capture_xcap()
                }
            },
            LinuxBackend::Xcap => capture_xcap(),
        }
    }

    fn try_x11() -> Result<Self, CaptureError> {
        let display = Display::primary().map_err(|error| CaptureError::Capture(error.to_string()))?;
        let width = display.width();
        let height = display.height();
        let capturer = Capturer::new(display).map_err(|error| CaptureError::Capture(error.to_string()))?;

        Ok(Self {
            backend: LinuxBackend::X11 {
                capturer,
                width,
                height,
            },
        })
    }

    fn try_wayland_wlroots() -> Result<Self, CaptureError> {
        let connection =
            WayshotConnection::new().map_err(|error| CaptureError::Capture(error.to_string()))?;
        let output = connection
            .get_all_outputs()
            .first()
            .ok_or(CaptureError::NoMonitors)?;
        let region = output.logical_region.clone();
        let width = region.inner.size.width;
        let height = region.inner.size.height;

        let probe = connection
            .screenshot(region.clone(), false)
            .map_err(|error| CaptureError::Capture(error.to_string()))?;
        if probe.width() == 0 || probe.height() == 0 {
            return Err(CaptureError::Capture(
                "wlroots probe capture returned an empty image".to_owned(),
            ));
        }

        Ok(Self {
            backend: LinuxBackend::WaylandWlroots {
                connection,
                region,
                width,
                height,
            },
        })
    }
}

fn is_wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|value| value.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false)
}

fn is_wlroots_desktop() -> bool {
    if std::env::var_os("SWAYSOCK").is_some()
        || std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
        || std::env::var_os("LABWC_PID").is_some()
    {
        return true;
    }

    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if desktop.contains("gnome")
        || desktop.contains("kde")
        || desktop.contains("plasma")
        || desktop.contains("ubuntu")
        || desktop.contains("pop")
    {
        return false;
    }

    desktop.contains("sway")
        || desktop.contains("hyprland")
        || desktop.contains("wlroots")
        || desktop.contains("river")
        || desktop.contains("labwc")
        || desktop.contains("wayfire")
}

fn capture_x11(
    capturer: &mut Capturer,
    width: u32,
    height: u32,
) -> Result<CapturedFrame, CaptureError> {
    let buffer = loop {
        match capturer.frame() {
            Ok(buffer) => break buffer,
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Err(error) => {
                return Err(CaptureError::Capture(error.to_string()));
            }
        }
    };

    let expected = width as usize * height as usize * 4;
    if buffer.len() != expected {
        return Err(CaptureError::Image(format!(
            "x11 frame length {} does not match {width}x{height}",
            buffer.len()
        )));
    }

    Ok(CapturedFrame {
        width,
        height,
        rgba: bgra_to_rgba(buffer),
    })
}

fn capture_wayland_wlroots(
    connection: &WayshotConnection,
    region: &LogicalRegion,
    width: u32,
    height: u32,
) -> Result<CapturedFrame, CaptureError> {
    let image = connection
        .screenshot(region.clone(), false)
        .map_err(|error| CaptureError::Capture(error.to_string()))?;
    let rgba = image.to_rgba8().into_raw();

    Ok(CapturedFrame {
        width,
        height,
        rgba,
    })
}

fn capture_xcap() -> Result<CapturedFrame, CaptureError> {
    let monitors = Monitor::all().map_err(|error| CaptureError::Capture(error.to_string()))?;
    let monitor = monitors.into_iter().next().ok_or(CaptureError::NoMonitors)?;
    let image = monitor
        .capture_image()
        .map_err(|error| CaptureError::Image(error.to_string()))?;
    Ok(CapturedFrame {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

fn bgra_to_rgba(buffer: &[u8]) -> Vec<u8> {
    let mut rgba = buffer.to_vec();
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    rgba
}
