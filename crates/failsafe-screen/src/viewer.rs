use std::sync::mpsc::Receiver;

use minifb::{Key, Window, WindowOptions};

use crate::decode::DecodedFrame;
use crate::monitor::ScreenError;

pub fn run_viewer(frame_rx: Receiver<DecodedFrame>) -> Result<(), ScreenError> {
    let mut width = 1280usize;
    let mut height = 720usize;
    let mut buffer = vec![0u32; width * height];

    let mut window = Window::new(
        "Failsafe Screen",
        width,
        height,
        WindowOptions::default(),
    )
    .map_err(|error| ScreenError::Capture(error.to_string()))?;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        while let Ok(frame) = frame_rx.try_recv() {
            width = frame.width as usize;
            height = frame.height as usize;
            buffer.resize(width * height, 0);
            for (pixel, chunk) in buffer.iter_mut().zip(frame.rgba.chunks_exact(4)) {
                *pixel = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32);
            }
        }

        window
            .update_with_buffer(&buffer, width, height)
            .map_err(|error| ScreenError::Capture(error.to_string()))?;
    }

    Ok(())
}
