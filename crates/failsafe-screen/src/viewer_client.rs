use std::path::Path;

use failsafe_core::control::{
    ControlError, ControlRequest, ControlResponse, control_socket_path, read_framed_payload,
    recv_response, send_request,
};
use failsafe_core::device::DeviceId;
use thiserror::Error;
use tokio::net::UnixStream;
use tokio::sync::mpsc;

#[derive(Debug, Error)]
pub enum ScreenViewerError {
    #[error("control error: {0}")]
    Control(#[from] ControlError),

    #[error("screen session closed")]
    Closed,
}

pub struct ScreenViewerClient {
    pub frames: mpsc::Receiver<Vec<u8>>,
}

impl ScreenViewerClient {
    pub async fn connect(target: DeviceId) -> Result<Self, ScreenViewerError> {
        Self::connect_at(&control_socket_path()?, target).await
    }

    pub async fn connect_at(
        path: impl AsRef<Path>,
        target: DeviceId,
    ) -> Result<Self, ScreenViewerError> {
        let mut stream = UnixStream::connect(path.as_ref()).await.map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound
                || error.kind() == std::io::ErrorKind::ConnectionRefused
            {
                ControlError::Config(
                    "daemon is not running; start it with `failsafe run`".to_owned(),
                )
            } else {
                ControlError::Io(error)
            }
        })?;

        send_request(
            &mut stream,
            &ControlRequest::OpenScreenShare { target },
        )
        .await?;

        match recv_response(&mut stream).await? {
            ControlResponse::Ready => {}
            ControlResponse::Error { message } => {
                return Err(ControlError::Config(message).into());
            }
        }

        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(async move {
            loop {
                match read_framed_payload(&mut stream).await {
                    Ok(frame) => {
                        if tx.send(frame).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self { frames: rx })
    }
}
