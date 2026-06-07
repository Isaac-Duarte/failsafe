use openh264::decoder::Decoder;
use openh264::formats::YUVSource;

use crate::monitor::ScreenError;

pub struct H264Decoder {
    decoder: Decoder,
}

impl H264Decoder {
    pub fn new() -> Result<Self, ScreenError> {
        let decoder = Decoder::new().map_err(|error| ScreenError::Decode(error.to_string()))?;
        Ok(Self { decoder })
    }

    pub fn decode_nal(&mut self, nal: &[u8]) -> Result<Option<DecodedFrame>, ScreenError> {
        let Some(yuv) = self
            .decoder
            .decode(nal)
            .map_err(|error| ScreenError::Decode(error.to_string()))?
        else {
            return Ok(None);
        };

        let (width, height) = yuv.dimensions();
        let mut rgba = vec![0u8; width * height * 4];
        yuv.write_rgba8(&mut rgba);
        Ok(Some(DecodedFrame {
            width: width as u32,
            height: height as u32,
            rgba,
        }))
    }
}

pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
