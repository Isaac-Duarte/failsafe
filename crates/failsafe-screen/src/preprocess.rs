use fast_image_resize::images::Image;
use fast_image_resize::{PixelType, ResizeAlg, ResizeOptions, Resizer};

pub struct FramePreprocessor {
    max_width: u32,
    resizer: Resizer,
    resize_options: ResizeOptions,
    cached_dst_width: u32,
    cached_dst_height: u32,
    dst_image: Option<Image<'static>>,
    dst_rgba_scratch: Vec<u8>,
    rgb_scratch: Vec<u8>,
}

impl FramePreprocessor {
    pub fn new(max_width: u32) -> Self {
        Self {
            max_width,
            resizer: Resizer::new(),
            resize_options: ResizeOptions {
                algorithm: ResizeAlg::Nearest,
                mul_div_alpha: false,
                ..ResizeOptions::default()
            },
            cached_dst_width: 0,
            cached_dst_height: 0,
            dst_image: None,
            dst_rgba_scratch: Vec::new(),
            rgb_scratch: Vec::new(),
        }
    }

    pub fn set_max_width(&mut self, max_width: u32) {
        if self.max_width != max_width {
            self.max_width = max_width;
            self.cached_dst_width = 0;
            self.cached_dst_height = 0;
            self.dst_image = None;
        }
    }

    pub fn rgba_to_rgb(
        &mut self,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let expected = width as usize * height as usize * 4;
        if rgba.len() != expected {
            return Err(format!(
                "rgba buffer length {} does not match {width}x{height}",
                rgba.len()
            ));
        }

        let (dst_width, dst_height) = output_dimensions(width, height, self.max_width);
        if dst_width == width && dst_height == height {
            strip_alpha_into(&mut self.rgb_scratch, &rgba);
            return Ok(());
        }

        self.ensure_dst_image(dst_width, dst_height);

        let src = Image::from_vec_u8(width, height, rgba, PixelType::U8x4)
            .map_err(|error| error.to_string())?;
        let dst = self.dst_image.as_mut().expect("dst image");

        self.resizer
            .resize(&src, dst, Some(&self.resize_options))
            .map_err(|error| error.to_string())?;

        let rgba = dst.buffer();
        self.rgb_scratch.clear();
        self.rgb_scratch.reserve(rgba.len() / 4 * 3);
        for pixel in rgba.chunks_exact(4) {
            self.rgb_scratch.extend_from_slice(&pixel[..3]);
        }
        Ok(())
    }

    pub fn rgb_pixels(&self) -> &[u8] {
        &self.rgb_scratch
    }

    fn ensure_dst_image(&mut self, dst_width: u32, dst_height: u32) {
        if self.cached_dst_width == dst_width && self.cached_dst_height == dst_height {
            return;
        }

        self.dst_image = Some(Image::new(dst_width, dst_height, PixelType::U8x4));
        self.cached_dst_width = dst_width;
        self.cached_dst_height = dst_height;
    }
}

pub fn output_dimensions(width: u32, height: u32, max_width: u32) -> (u32, u32) {
    if width <= max_width {
        return (width, height);
    }
    let dst_width = max_width;
    let dst_height = ((height as u64 * max_width as u64) / width as u64)
        .try_into()
        .unwrap_or(height);
    (dst_width, dst_height.max(1))
}

fn strip_alpha_into(rgb: &mut Vec<u8>, rgba: &[u8]) {
    rgb.clear();
    rgb.reserve(rgba.len() / 4 * 3);
    for pixel in rgba.chunks_exact(4) {
        rgb.extend_from_slice(&pixel[..3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_dimensions_preserves_aspect_ratio() {
        let (w, h) = output_dimensions(3840, 2160, 1280);
        assert_eq!(w, 1280);
        assert_eq!(h, 720);
    }

    #[test]
    fn output_dimensions_skips_downscale_when_small_enough() {
        let (w, h) = output_dimensions(1280, 720, 1920);
        assert_eq!((w, h), (1280, 720));
    }

    #[test]
    fn set_max_width_clears_cached_resize_buffers() {
        let mut preprocessor = FramePreprocessor::new(1280);
        preprocessor.set_max_width(640);
        let (width, height) = output_dimensions(3840, 2160, 640);
        assert_eq!((width, height), (640, 360));
    }

    #[test]
    fn downscales_large_frame() {
        let mut preprocessor = FramePreprocessor::new(1280);
        let width = 3840u32;
        let height = 2160u32;
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        for y in 0..height {
            for x in 0..width {
                let index = ((y * width + x) * 4) as usize;
                rgba[index] = (x % 256) as u8;
                rgba[index + 1] = (y % 256) as u8;
                rgba[index + 2] = 128;
                rgba[index + 3] = 255;
            }
        }

        preprocessor
            .rgba_to_rgb(rgba, width, height)
            .expect("preprocess");
        assert_eq!(preprocessor.rgb_pixels().len(), 1280 * 720 * 3);
        assert_eq!(preprocessor.rgb_pixels()[2], 128);
    }

    #[test]
    fn reuses_rgb_buffer_across_frames() {
        let mut preprocessor = FramePreprocessor::new(1280);
        let rgba = vec![255u8; 4 * 4 * 4];
        preprocessor.rgba_to_rgb(rgba, 4, 4).expect("first");
        let first_ptr = preprocessor.rgb_pixels().as_ptr();
        preprocessor
            .rgba_to_rgb(vec![0u8; 4 * 4 * 4], 4, 4)
            .expect("second");
        assert_eq!(preprocessor.rgb_pixels().as_ptr(), first_ptr);
    }
}
