use std::collections::HashMap;
use std::time::Duration;

use failsafe_transport::iroh::DesktopSession;
use failsafe_transport::transport::TransportError;
use image::ImageReader;
use iroh::endpoint::{RecvStream, SendStream};
use minifb::{Key, MouseButton, Window, WindowOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tracing::warn;

use crate::keys::key_to_code;
use crate::protocol::{
    FrameKind, encode_key, encode_mouse_button, encode_mouse_move, parse_frame_kind,
};

const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

pub async fn run_desktop_viewer_view_only(session: DesktopSession) -> Result<(), TransportError> {
    run_desktop_viewer_loop(session, None).await
}

pub async fn run_desktop_viewer_with_input(
    session: DesktopSession,
    mut input_send: SendStream,
) -> Result<(), TransportError> {
    run_desktop_viewer_loop(session, Some(&mut input_send)).await
}

async fn run_desktop_viewer_loop(
    mut session: DesktopSession,
    mut input_send: Option<&mut SendStream>,
) -> Result<(), TransportError> {
    let title = format!("Failsafe Desktop — {}", session.from);
    let mut window = Window::new(&title, 1280, 720, WindowOptions::default())
        .map_err(|error| TransportError::Codec(error.to_string()))?;

    let (frame_tx, mut frame_rx) = mpsc::channel::<Vec<u8>>(8);
    let mut recv = session.recv;
    let frame_task = tokio::spawn(async move {
        while let Ok(Some(payload)) = read_length_prefixed(&mut recv).await {
            if frame_tx.send(payload).await.is_err() {
                break;
            }
        }
    });

    let mut remote_width = 0u32;
    let mut remote_height = 0u32;
    let mut buffer = Vec::new();
    let mut last_mouse: Option<(i32, i32)> = None;
    let mut buttons_down = [false; 3];
    let mut key_state = HashMap::new();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        while let Ok(payload) = frame_rx.try_recv() {
            match parse_frame_kind(&payload) {
                FrameKind::Config { width, height } => {
                    remote_width = width;
                    remote_height = height;
                }
                FrameKind::Jpeg => {
                    if payload.len() >= 5 {
                        let jpeg_len =
                            u32::from_be_bytes(payload[1..5].try_into().expect("jpeg len")) as usize;
                        if payload.len() >= 5 + jpeg_len {
                            if let Ok((width, height, pixels)) =
                                decode_jpeg_to_buffer(&payload[5..5 + jpeg_len])
                            {
                                buffer = pixels;
                                if window.get_size().0 != width as usize
                                    || window.get_size().1 != height as usize
                                {
                                    window = Window::new(
                                        &title,
                                        width as usize,
                                        height as usize,
                                        WindowOptions::default(),
                                    )
                                    .map_err(|error| TransportError::Codec(error.to_string()))?;
                                }
                                let _ = window.update_with_buffer(
                                    &buffer,
                                    width as usize,
                                    height as usize,
                                );
                            }
                        }
                    }
                }
                FrameKind::Unknown(kind) => warn!("unknown frame kind {kind}"),
            }
        }

        if remote_width > 0 && remote_height > 0 {
            if let Some(send) = input_send.as_mut() {
                relay_input(
                    send,
                    &window,
                    remote_width,
                    remote_height,
                    &mut last_mouse,
                    &mut buttons_down,
                    &mut key_state,
                )
                .await?;
            }
        }

        window.update();
        tokio::time::sleep(Duration::from_millis(16)).await;
    }

    frame_task.abort();
    let _ = session.send.finish();
    if let Some(send) = input_send {
        let _ = send.finish();
    }
    Ok(())
}

async fn read_length_prefixed(recv: &mut RecvStream) -> Result<Option<Vec<u8>>, TransportError> {
    let mut len_buf = [0u8; 4];
    match recv.read_exact(&mut len_buf).await {
        Ok(()) => {}
        Err(error) => {
            if error.to_string().contains("closed") || error.to_string().contains("reset") {
                return Ok(None);
            }
            return Err(TransportError::Codec(error.to_string()));
        }
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(TransportError::Codec("frame too large".to_owned()));
    }
    let mut payload = vec![0u8; len];
    recv.read_exact(&mut payload)
        .await
        .map_err(|error| TransportError::Codec(error.to_string()))?;
    Ok(Some(payload))
}

fn decode_jpeg_to_buffer(jpeg: &[u8]) -> Result<(u32, u32, Vec<u32>), String> {
    let image = ImageReader::new(std::io::Cursor::new(jpeg))
        .with_guessed_format()
        .map_err(|error| error.to_string())?
        .decode()
        .map_err(|error| error.to_string())?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let buffer: Vec<u32> = rgba
        .chunks_exact(4)
        .map(|px| {
            let r = u32::from(px[0]);
            let g = u32::from(px[1]);
            let b = u32::from(px[2]);
            (r << 16) | (g << 8) | b
        })
        .collect();
    Ok((width, height, buffer))
}

async fn relay_input(
    send: &mut SendStream,
    window: &Window,
    remote_width: u32,
    remote_height: u32,
    last_mouse: &mut Option<(i32, i32)>,
    buttons_down: &mut [bool; 3],
    key_state: &mut HashMap<u32, bool>,
) -> Result<(), TransportError> {
    let (win_w, win_h) = window.get_size();
    if win_w == 0 || win_h == 0 {
        return Ok(());
    }

    if let Some((x, y)) = window.get_mouse_pos(minifb::MouseMode::Clamp) {
        let remote_x =
            (x as u32 * remote_width / win_w as u32).min(remote_width.saturating_sub(1)) as i32;
        let remote_y =
            (y as u32 * remote_height / win_h as u32).min(remote_height.saturating_sub(1)) as i32;
        if last_mouse != &Some((remote_x, remote_y)) {
            *last_mouse = Some((remote_x, remote_y));
            write_input(send, &encode_mouse_move(remote_x, remote_y)).await?;
        }
    }

    for (index, button) in [MouseButton::Left, MouseButton::Right, MouseButton::Middle]
        .into_iter()
        .enumerate()
    {
        let pressed = window.get_mouse_down(button);
        if pressed != buttons_down[index] {
            buttons_down[index] = pressed;
            write_input(send, &encode_mouse_button(index as u8, pressed)).await?;
        }
    }

    for key in tracked_keys() {
        let pressed = window.is_key_down(*key);
        let Some(code) = key_to_code(*key) else {
            continue;
        };
        let was_pressed = key_state.get(&code).copied().unwrap_or(false);
        if pressed != was_pressed {
            key_state.insert(code, pressed);
            write_input(send, &encode_key(code, pressed)).await?;
        }
    }

    Ok(())
}

async fn write_input(send: &mut SendStream, message: &[u8]) -> Result<(), TransportError> {
    send.write_all(message)
        .await
        .map_err(|error| TransportError::Codec(error.to_string()))?;
    Ok(())
}

fn tracked_keys() -> &'static [Key] {
    &[
        Key::A,
        Key::B,
        Key::C,
        Key::D,
        Key::E,
        Key::F,
        Key::G,
        Key::H,
        Key::I,
        Key::J,
        Key::K,
        Key::L,
        Key::M,
        Key::N,
        Key::O,
        Key::P,
        Key::Q,
        Key::R,
        Key::S,
        Key::T,
        Key::U,
        Key::V,
        Key::W,
        Key::X,
        Key::Y,
        Key::Z,
        Key::Key0,
        Key::Key1,
        Key::Key2,
        Key::Key3,
        Key::Key4,
        Key::Key5,
        Key::Key6,
        Key::Key7,
        Key::Key8,
        Key::Key9,
        Key::Space,
        Key::Enter,
        Key::Backspace,
        Key::Tab,
        Key::Left,
        Key::Right,
        Key::Up,
        Key::Down,
        Key::LeftShift,
        Key::LeftCtrl,
        Key::LeftAlt,
    ]
}
