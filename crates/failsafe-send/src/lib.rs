mod coordinator;
mod feature;
mod files;
mod inbound;
mod notify;
mod outbound;
mod payload;

#[cfg(test)]
mod integration_tests;

pub use coordinator::SendCoordinator;
pub use feature::SendFeature;
pub use files::{collect_file_preview, format_bytes, read_files_from_paths};
pub use inbound::save_received_files;
pub use outbound::prepare_send_payload;
pub use payload::{encode_envelope, SendAck, SendEnvelope, SendPayload};
