use failsafe_core::screen::ScreenInfo;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScreenError {
    #[error("screen capture is not supported on this platform")]
    UnsupportedPlatform,

    #[error("screen sharing on Linux requires Wayland")]
    WaylandRequired,

    #[error("screen capture is not supported on this system")]
    NotSupported,

    #[error("screen capture permission was denied")]
    PermissionDenied,

    #[error("screen capture is not available; build with the scap-capture feature")]
    CaptureUnavailable,

    #[error("screen capture failed: {0}")]
    Capture(String),

    #[error("encode failed: {0}")]
    Encode(String),

    #[error("decode failed: {0}")]
    Decode(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("transport error: {0}")]
    Transport(String),
}

pub fn ensure_capture_platform() -> Result<(), ScreenError> {
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WAYLAND_DISPLAY").is_err() {
            return Err(ScreenError::WaylandRequired);
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        return Err(ScreenError::UnsupportedPlatform);
    }

    Ok(())
}

#[cfg(not(feature = "scap-capture"))]
pub fn list_displays() -> Result<Vec<ScreenInfo>, ScreenError> {
    let _ = ensure_capture_platform()?;
    Err(ScreenError::CaptureUnavailable)
}

#[cfg(feature = "scap-capture")]
pub fn list_displays() -> Result<Vec<ScreenInfo>, ScreenError> {
    ensure_capture_platform()?;

    if !scap::is_supported() {
        return Err(ScreenError::NotSupported);
    }

    if !scap::has_permission() && !scap::request_permission() {
        return Err(ScreenError::PermissionDenied);
    }

    let targets = scap::get_all_targets();
    let mut displays = Vec::new();
    for target in targets {
        let scap::Target::Display(display) = target else {
            continue;
        };
        let (width, height) = display_dimensions(&display);
        displays.push(ScreenInfo {
            id: displays.len() as u32,
            name: display.title,
            width,
            height,
        });
    }

    #[cfg(target_os = "linux")]
    if displays.is_empty() {
        displays.push(ScreenInfo {
            id: 0,
            name: "Display (confirm in portal)".to_owned(),
            width: 0,
            height: 0,
        });
    }

    if displays.is_empty() {
        return Err(ScreenError::Capture(
            "no captureable displays found".to_owned(),
        ));
    }

    Ok(displays)
}

#[cfg(all(feature = "scap-capture", target_os = "macos"))]
fn display_dimensions(display: &scap::Display) -> (u32, u32) {
    let target = scap::Target::Display(display.clone());
    let (width, height) = scap::get_target_dimensions(&target);
    (width as u32, height as u32)
}

#[cfg(all(feature = "scap-capture", target_os = "linux"))]
fn display_dimensions(_display: &scap::Display) -> (u32, u32) {
    (0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_list_json_matches_transport_contract() {
        let screens = vec![
            ScreenInfo {
                id: 0,
                name: "Built-in".to_owned(),
                width: 1920,
                height: 1080,
            },
            ScreenInfo {
                id: 1,
                name: "External".to_owned(),
                width: 2560,
                height: 1440,
            },
        ];
        let json = serde_json::to_vec(&screens).expect("serialize screens");
        let len = u32::try_from(json.len()).expect("length fits u32");
        assert_eq!(len.to_be_bytes().len(), 4);
        let decoded: Vec<ScreenInfo> = serde_json::from_slice(&json).expect("deserialize screens");
        assert_eq!(decoded, screens);
    }
}
