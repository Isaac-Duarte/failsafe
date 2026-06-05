use image::{DynamicImage, ImageBuffer, RgbaImage};

use crate::io::ImageDataOwned;

pub(super) fn encode_image_png(image: &ImageDataOwned) -> Result<Vec<u8>, String> {
    let buffer: RgbaImage = ImageBuffer::from_raw(image.width, image.height, image.rgba.clone())
        .ok_or_else(|| "invalid clipboard image dimensions".to_owned())?;
    let mut encoded = Vec::new();
    DynamicImage::ImageRgba8(buffer)
        .write_to(
            &mut std::io::Cursor::new(&mut encoded),
            image::ImageFormat::Png,
        )
        .map_err(|error| error.to_string())?;
    Ok(encoded)
}

pub(super) fn decode_image_png(data: &[u8]) -> Result<ImageDataOwned, String> {
    let image = image::load_from_memory(data).map_err(|error| error.to_string())?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok(ImageDataOwned {
        width,
        height,
        rgba: rgba.into_raw(),
    })
}
