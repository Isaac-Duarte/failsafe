use failsafe_core::control::ControlError;
use failsafe_core::feature::FeatureError;
use failsafe_transport::transport::TransportError;

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("feature error: {0}")]
    Feature(#[from] FeatureError),

    #[error("control error: {0}")]
    Control(#[from] ControlError),

    #[error("transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("transport `{0}` is not available yet")]
    TransportUnavailable(String),

    #[error(
        "this device was removed from your account; run `failsafe pair --code <CODE>` to re-add it"
    )]
    DeviceRemoved,
}
