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

    #[error("transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("transport `{0}` is not available yet")]
    TransportUnavailable(String),
}
