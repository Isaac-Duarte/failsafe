use enigo::{
    Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings,
};
use failsafe_transport::iroh::InputSession;
use iroh::endpoint::RecvStream;
use tracing::{debug, warn};

use crate::keys::code_to_key;
use crate::protocol::{INPUT_KEY, INPUT_MOUSE_BUTTON, INPUT_MOUSE_MOVE};

pub async fn run_input_host(session: InputSession) {
    let device = session.from;
    debug!(%device, "starting input host");

    let mut enigo = match Enigo::new(&Settings::default()) {
        Ok(enigo) => enigo,
        Err(error) => {
            warn!(%device, "failed to initialize input injection: {error}");
            return;
        }
    };

    if let Err(error) = apply_input_stream(&mut enigo, session.recv).await {
        warn!(%device, "input host ended: {error}");
    }
}

async fn apply_input_stream(enigo: &mut Enigo, mut recv: RecvStream) -> Result<(), String> {
    let mut buf = [0u8; 16];
    loop {
        let read = recv
            .read(&mut buf)
            .await
            .map_err(|error| error.to_string())?;
        let Some(read) = read else {
            break;
        };
        if read == 0 {
            break;
        }
        apply_input_message(enigo, &buf[..read])?;
    }
    Ok(())
}

fn apply_input_message(enigo: &mut Enigo, data: &[u8]) -> Result<(), String> {
    match data.first().copied() {
        Some(INPUT_MOUSE_MOVE) if data.len() >= 9 => {
            let x = i32::from_be_bytes(data[1..5].try_into().expect("x"));
            let y = i32::from_be_bytes(data[5..9].try_into().expect("y"));
            enigo
                .move_mouse(x, y, Coordinate::Abs)
                .map_err(|error| error.to_string())?;
        }
        Some(INPUT_MOUSE_BUTTON) if data.len() >= 3 => {
            let button = match data[1] {
                0 => Button::Left,
                1 => Button::Right,
                2 => Button::Middle,
                _ => return Ok(()),
            };
            let direction = if data[2] != 0 {
                Direction::Press
            } else {
                Direction::Release
            };
            enigo
                .button(button, direction)
                .map_err(|error| error.to_string())?;
        }
        Some(INPUT_KEY) if data.len() >= 6 => {
            let key_code = u32::from_be_bytes(data[1..5].try_into().expect("key"));
            let direction = if data[5] != 0 {
                Direction::Press
            } else {
                Direction::Release
            };
            if let Some(mfb_key) = code_to_key(key_code) {
                if let Some(key) = minifb_key_to_enigo(mfb_key) {
                    enigo
                        .key(key, direction)
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn minifb_key_to_enigo(key: minifb::Key) -> Option<Key> {
    use minifb::Key as MfbKey;
    Some(match key {
        MfbKey::A => Key::Unicode('a'),
        MfbKey::B => Key::Unicode('b'),
        MfbKey::C => Key::Unicode('c'),
        MfbKey::D => Key::Unicode('d'),
        MfbKey::E => Key::Unicode('e'),
        MfbKey::F => Key::Unicode('f'),
        MfbKey::G => Key::Unicode('g'),
        MfbKey::H => Key::Unicode('h'),
        MfbKey::I => Key::Unicode('i'),
        MfbKey::J => Key::Unicode('j'),
        MfbKey::K => Key::Unicode('k'),
        MfbKey::L => Key::Unicode('l'),
        MfbKey::M => Key::Unicode('m'),
        MfbKey::N => Key::Unicode('n'),
        MfbKey::O => Key::Unicode('o'),
        MfbKey::P => Key::Unicode('p'),
        MfbKey::Q => Key::Unicode('q'),
        MfbKey::R => Key::Unicode('r'),
        MfbKey::S => Key::Unicode('s'),
        MfbKey::T => Key::Unicode('t'),
        MfbKey::U => Key::Unicode('u'),
        MfbKey::V => Key::Unicode('v'),
        MfbKey::W => Key::Unicode('w'),
        MfbKey::X => Key::Unicode('x'),
        MfbKey::Y => Key::Unicode('y'),
        MfbKey::Z => Key::Unicode('z'),
        MfbKey::Key0 => Key::Unicode('0'),
        MfbKey::Key1 => Key::Unicode('1'),
        MfbKey::Key2 => Key::Unicode('2'),
        MfbKey::Key3 => Key::Unicode('3'),
        MfbKey::Key4 => Key::Unicode('4'),
        MfbKey::Key5 => Key::Unicode('5'),
        MfbKey::Key6 => Key::Unicode('6'),
        MfbKey::Key7 => Key::Unicode('7'),
        MfbKey::Key8 => Key::Unicode('8'),
        MfbKey::Key9 => Key::Unicode('9'),
        MfbKey::Space => Key::Space,
        MfbKey::Enter => Key::Return,
        MfbKey::Escape => Key::Escape,
        MfbKey::Backspace => Key::Backspace,
        MfbKey::Tab => Key::Tab,
        MfbKey::Left => Key::LeftArrow,
        MfbKey::Right => Key::RightArrow,
        MfbKey::Up => Key::UpArrow,
        MfbKey::Down => Key::DownArrow,
        MfbKey::LeftShift => Key::Shift,
        MfbKey::LeftCtrl => Key::Control,
        MfbKey::LeftAlt => Key::Alt,
        _ => return None,
    })
}
