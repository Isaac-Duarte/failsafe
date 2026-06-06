use std::path::Path;

use failsafe_core::control::{
    ControlError, ControlRequest, ControlResponse, control_socket_path, recv_response, send_request,
};
use failsafe_core::device::DeviceId;
use thiserror::Error;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

use crate::protocol::{
    PACKET_TAG_CONTROL, PACKET_TAG_FRAME, ProtocolError, encode_set_quality, read_tagged_packet,
};
use crate::quality::ScreenQualityPreset;

#[derive(Debug, Error)]
pub enum ScreenViewerError {
    #[error("control error: {0}")]
    Control(#[from] ControlError),

    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("screen session closed")]
    Closed,
}

pub struct ScreenViewerClient {
    pub frames: mpsc::Receiver<Vec<u8>>,
    control_tx: mpsc::Sender<ScreenQualityPreset>,
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

        send_request(&mut stream, &ControlRequest::OpenScreenShare { target }).await?;

        match recv_response(&mut stream).await? {
            ControlResponse::Ready => {}
            ControlResponse::Error { message } => {
                return Err(ControlError::Config(message).into());
            }
        }

        let (mut stream_read, mut stream_write) = stream.into_split();

        let (frame_tx, frame_rx) = mpsc::channel(8);
        let (control_tx, mut control_rx) = mpsc::channel(8);

        tokio::spawn(async move {
            let mut reader = BufReader::new(&mut stream_read);
            loop {
                match read_tagged_packet(&mut reader).await {
                    Ok((PACKET_TAG_FRAME, jpeg)) => {
                        if frame_tx.send(jpeg).await.is_err() {
                            break;
                        }
                    }
                    Ok((PACKET_TAG_CONTROL, _)) => {}
                    Ok((tag, _)) => {
                        tracing::warn!("unexpected screen packet tag from daemon: {tag}");
                    }
                    Err(ProtocolError::Io(error))
                        if error.kind() == std::io::ErrorKind::UnexpectedEof =>
                    {
                        break;
                    }
                    Err(error) => {
                        tracing::warn!("screen viewer read failed: {error}");
                        break;
                    }
                }
            }
        });

        tokio::spawn(async move {
            while let Some(preset) = control_rx.recv().await {
                match encode_set_quality(preset) {
                    Ok(packet) => {
                        if stream_write.write_all(&packet).await.is_err() {
                            break;
                        }
                        if stream_write.flush().await.is_err() {
                            break;
                        }
                    }
                    Err(error) => tracing::warn!("failed to encode screen quality: {error}"),
                }
            }
        });

        Ok(Self {
            frames: frame_rx,
            control_tx,
        })
    }

    pub fn control_sender(&self) -> mpsc::Sender<ScreenQualityPreset> {
        self.control_tx.clone()
    }

    pub async fn set_quality(&self, preset: ScreenQualityPreset) -> Result<(), ScreenViewerError> {
        self.control_tx
            .send(preset)
            .await
            .map_err(|_| ScreenViewerError::Closed)
    }
}
