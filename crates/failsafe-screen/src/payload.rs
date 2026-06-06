use serde::Serialize;
use specta::Type;

#[derive(Clone, Serialize, Type)]
pub struct ScreenFramePayload {
    pub jpeg: Vec<u8>,
}
