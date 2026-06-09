use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder};
use thiserror::Error;
use xcap::Monitor;

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("screen capture failed: {0}")]
    Platform(String),

    #[error("encode failed: {0}")]
    Encode(String),
}

pub fn list_displays() -> Result<Vec<(u32, String)>, CaptureError> {
    Monitor::all()
        .map_err(|error| CaptureError::Platform(error.to_string()))
        .map(|monitors| {
            monitors
                .into_iter()
                .enumerate()
                .map(|(index, monitor)| {
                    let name = monitor.name().to_owned();
                    (index as u32, name)
                })
                .collect()
        })
}

pub fn capture_display_jpeg(display_index: u32, quality: u8) -> Result<(Vec<u8>, u32, u32), CaptureError> {
    let monitors = Monitor::all().map_err(|error| CaptureError::Platform(error.to_string()))?;
    let monitor = monitors
        .into_iter()
        .nth(display_index as usize)
        .ok_or_else(|| CaptureError::Platform(format!("display index {display_index} not found")))?;

    let width = monitor.width();
    let height = monitor.height();
    let image = monitor
        .capture_image()
        .map_err(|error| CaptureError::Platform(error.to_string()))?;

    let rgba = image.into_raw();
    let mut jpeg = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut jpeg, quality);
    encoder
        .write_image(&rgba, width, height, ExtendedColorType::Rgba8)
        .map_err(|error| CaptureError::Encode(error.to_string()))?;

    Ok((jpeg, width, height))
}
