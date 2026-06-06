use std::collections::HashMap;
use std::io::{pipe, Read};

use zbus::blocking::{Connection, Proxy};
use zvariant::{OwnedFd, OwnedValue, Value};

use super::{CaptureError, CapturedFrame};

const KWIN_SERVICE: &str = "org.kde.KWin";
const KWIN_PATH: &str = "/org/kde/KWin/ScreenShot2";
const KWIN_INTERFACE: &str = "org.kde.KWin.ScreenShot2";

pub struct KdeKwinCapturer {
    connection: Connection,
}

impl KdeKwinCapturer {
    pub fn try_new() -> Result<Self, CaptureError> {
        let connection =
            Connection::session().map_err(|error| CaptureError::Capture(error.to_string()))?;
        let capturer = Self { connection };
        capturer.capture()?;
        Ok(capturer)
    }

    pub fn capture(&self) -> Result<CapturedFrame, CaptureError> {
        capture_via_kwin(&self.connection)
    }
}

fn capture_via_kwin(connection: &Connection) -> Result<CapturedFrame, CaptureError> {
    let proxy = Proxy::new(connection, KWIN_SERVICE, KWIN_PATH, KWIN_INTERFACE)
        .map_err(|error| CaptureError::Capture(error.to_string()))?;

    let (mut read_end, write_end) =
        pipe().map_err(|error| CaptureError::Capture(error.to_string()))?;

    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("include-cursor", Value::Bool(false));

    let reply: HashMap<String, OwnedValue> = proxy
        .call(
            "CaptureActiveScreen",
            &(options, OwnedFd::from(std::os::fd::OwnedFd::from(write_end))),
        )
        .map_err(|error| CaptureError::Capture(error.to_string()))?;

    let width = map_value_u32(
        reply
            .get("width")
            .ok_or_else(|| CaptureError::Capture("kwin screenshot missing width".to_owned()))?,
    )?;
    let height = map_value_u32(
        reply
            .get("height")
            .ok_or_else(|| CaptureError::Capture("kwin screenshot missing height".to_owned()))?,
    )?;
    let stride = map_value_u32(
        reply
            .get("stride")
            .ok_or_else(|| CaptureError::Capture("kwin screenshot missing stride".to_owned()))?,
    )?;

    let mut data = Vec::new();
    read_end
        .read_to_end(&mut data)
        .map_err(|error| CaptureError::Capture(error.to_string()))?;

    if data.is_empty() {
        return Err(CaptureError::Capture(
            "kwin screenshot returned no pixel data".to_owned(),
        ));
    }

    let rgba = bgra_stride_to_rgba(&data, width, height, stride);
    Ok(CapturedFrame {
        width,
        height,
        rgba,
    })
}

fn map_value_u32(value: &OwnedValue) -> Result<u32, CaptureError> {
    value
        .downcast_ref::<u32>()
        .map_err(|err| CaptureError::Capture(format!("expected u32 metadata, got {value:?} {err}")))
}

/// KWin returns premultiplied BGRA32 with row padding (`stride` may exceed `width * 4`).
fn bgra_stride_to_rgba(data: &[u8], width: u32, height: u32, stride: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        let row_start = (y * stride) as usize;
        for x in 0..width {
            let src = row_start + (x as usize) * 4;
            let dst = ((y * width + x) * 4) as usize;
            rgba[dst] = data[src + 2];
            rgba[dst + 1] = data[src + 1];
            rgba[dst + 2] = data[src];
            rgba[dst + 3] = data[src + 3];
        }
    }
    rgba
}
