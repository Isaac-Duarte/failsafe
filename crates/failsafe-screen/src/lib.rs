mod capture;
mod host;
mod payload;
mod preprocess;
mod protocol;
mod quality;
mod relay;
mod viewer_client;

pub use capture::CaptureError;
pub use host::{ScreenHostError, run_screen_host};
pub use protocol::{
    PACKET_TAG_CONTROL, PACKET_TAG_FRAME, ProtocolError, SCREEN_HANDSHAKE, SCREEN_INIT_LEN,
    build_screen_init, decode_control, decode_frame, encode_control, encode_frame,
    encode_set_quality, encode_tagged_packet, is_screen_handshake, read_tagged_packet,
    write_tagged_packet,
};
pub use payload::ScreenFramePayload;
pub use quality::{ScreenControlMessage, ScreenQualityPreset, ScreenSettings};
pub use relay::relay_tagged_bidirectional;
pub use viewer_client::{ScreenViewerClient, ScreenViewerError};
