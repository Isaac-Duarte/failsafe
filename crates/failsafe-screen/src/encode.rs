use openh264::encoder::{BitRate, Encoder, EncoderConfig, RateControlMode};
use openh264::formats::{RgbSliceU8, YUVBuffer};
use openh264::OpenH264API;

use crate::monitor::ScreenError;

pub struct H264Encoder {
    encoder: Encoder,
    width: u32,
    height: u32,
}

impl H264Encoder {
    pub fn new(width: u32, height: u32) -> Result<Self, ScreenError> {
        let api = OpenH264API::from_source();
        let config = EncoderConfig::new()
            .bitrate(BitRate::from_bps(2_000_000))
            .rate_control_mode(RateControlMode::Bitrate);
        let encoder = Encoder::with_api_config(api, config)
            .map_err(|error| ScreenError::Encode(error.to_string()))?;
        Ok(Self {
            encoder,
            width,
            height,
        })
    }

    pub fn encode_bgra(&mut self, width: u32, height: u32, bgra: &[u8]) -> Result<Vec<u8>, ScreenError> {
        if width != self.width || height != self.height {
            self.width = width;
            self.height = height;
        }
        let rgb = bgra_to_rgb(bgra);
        let rgb_source = RgbSliceU8::new(&rgb, (width as usize, height as usize));
        let yuv = YUVBuffer::from_rgb8_source(rgb_source);
        let bitstream = self
            .encoder
            .encode(&yuv)
            .map_err(|error| ScreenError::Encode(error.to_string()))?;
        Ok(bitstream.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::H264Decoder;

    #[test]
    fn encodes_and_decodes_a_frame() {
        let width = 64u32;
        let height = 64u32;
        let mut bgra = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                bgra.push((x * 4) as u8);
                bgra.push((y * 4) as u8);
                bgra.push(128);
                bgra.push(255);
            }
        }

        let mut encoder = H264Encoder::new(width, height).expect("encoder");
        let mut decoder = H264Decoder::new().expect("decoder");
        let mut decoded = None;
        for _ in 0..8 {
            let nal = encoder
                .encode_bgra(width, height, &bgra)
                .expect("encode frame");
            if let Some(frame) = decoder.decode_nal(&nal).expect("decode frame") {
                decoded = Some(frame);
            }
        }

        let frame = decoded.expect("decoded frame");
        assert_eq!(frame.width, width);
        assert_eq!(frame.height, height);
        assert_eq!(frame.rgba.len(), (width * height * 4) as usize);
    }
}

fn bgra_to_rgb(bgra: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(bgra.len() / 4 * 3);
    for chunk in bgra.chunks_exact(4) {
        rgb.push(chunk[2]);
        rgb.push(chunk[1]);
        rgb.push(chunk[0]);
    }
    rgb
}
