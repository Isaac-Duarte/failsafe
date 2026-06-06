use std::sync::Arc;

use failsafe_core::device::DeviceId;
use failsafe_core::message::FeatureMessage;
use iroh::endpoint::{RecvStream, SendStream};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, warn};

use crate::codec;
use crate::shell;
use crate::transport::TransportError;

const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

pub type ShellAcceptor = mpsc::Sender<ShellSession>;
pub type SharedShellAcceptor = Arc<Mutex<Option<ShellAcceptor>>>;

pub struct ShellSession {
    pub from: DeviceId,
    pub rows: u16,
    pub cols: u16,
    pub send: SendStream,
    pub recv: RecvStream,
}

pub async fn read_exact(recv: &mut RecvStream, buf: &mut [u8]) -> Result<(), TransportError> {
    let mut offset = 0;
    while offset < buf.len() {
        let read = recv
            .read(&mut buf[offset..])
            .await
            .map_err(|error| TransportError::Codec(error.to_string()))?;
        let Some(read) = read else {
            return Err(TransportError::Codec(
                "stream closed before read complete".to_owned(),
            ));
        };
        if read == 0 {
            return Err(TransportError::Codec(
                "stream closed before read complete".to_owned(),
            ));
        }
        offset += read;
    }
    Ok(())
}

pub async fn handle_incoming_bi_stream(
    send: SendStream,
    mut recv: RecvStream,
    device: DeviceId,
    inbox: mpsc::Sender<FeatureMessage>,
    shell_acceptor: SharedShellAcceptor,
) {
    let mut header = [0u8; 4];
    if let Err(error) = read_exact(&mut recv, &mut header).await {
        warn!(%device, "failed to read stream header: {error}");
        return;
    }

    if shell::is_shell_handshake(&header) {
        let mut size_buf = [0u8; 4];
        if let Err(error) = read_exact(&mut recv, &mut size_buf).await {
            warn!(%device, "shell stream missing terminal size: {error}");
            return;
        }
        let rows = u16::from_be_bytes(size_buf[..2].try_into().expect("rows"));
        let cols = u16::from_be_bytes(size_buf[2..].try_into().expect("cols"));

        let acceptor = shell_acceptor.lock().await.clone();
        let Some(acceptor) = acceptor else {
            warn!(%device, "rejected shell stream: shell acceptor not registered");
            return;
        };

        let session = ShellSession {
            from: device,
            rows,
            cols,
            send,
            recv,
        };

        if acceptor.send(session).await.is_err() {
            warn!(%device, "shell acceptor closed");
        }
        return;
    }

    let length = u32::from_be_bytes(header) as usize;
    if length > MAX_MESSAGE_SIZE {
        warn!(%device, "feature frame exceeds maximum size");
        return;
    }

    let mut payload = vec![0u8; length];
    if let Err(error) = read_exact(&mut recv, &mut payload).await {
        warn!(%device, "failed to read feature frame: {error}");
        return;
    }

    let mut frame = header.to_vec();
    frame.extend_from_slice(&payload);

    match codec::decode(&frame) {
        Ok(message) => {
            if inbox.send(message).await.is_err() {
                debug!("inbox closed while delivering message");
            }
        }
        Err(error) => warn!(%device, "failed to decode inbound frame: {error}"),
    }
}

pub async fn relay_shell_streams<R, W>(
    mut send: SendStream,
    mut recv: RecvStream,
    mut input: R,
    mut output: W,
) -> Result<(), TransportError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let input_to_stream = async {
        let mut buf = [0u8; 4096];
        loop {
            let read = input
                .read(&mut buf)
                .await
                .map_err(|error| TransportError::Codec(error.to_string()))?;
            if read == 0 {
                break;
            }
            send.write_all(&buf[..read])
                .await
                .map_err(|error| TransportError::Codec(error.to_string()))?;
        }
        send.finish()
            .map_err(|error| TransportError::Codec(error.to_string()))?;
        Ok::<(), TransportError>(())
    };

    let stream_to_output = async {
        let mut buf = [0u8; 4096];
        loop {
            let read = recv
                .read(&mut buf)
                .await
                .map_err(|error| TransportError::Codec(error.to_string()))?;
            let Some(read) = read else {
                break;
            };
            if read == 0 {
                break;
            }
            output
                .write_all(&buf[..read])
                .await
                .map_err(|error| TransportError::Codec(error.to_string()))?;
            output
                .flush()
                .await
                .map_err(|error| TransportError::Codec(error.to_string()))?;
        }
        Ok::<(), TransportError>(())
    };

    tokio::select! {
        result = input_to_stream => result?,
        result = stream_to_output => result?,
    }
    Ok(())
}

pub async fn relay_shell_to_channels(
    mut send: SendStream,
    mut recv: RecvStream,
    mut from_local: mpsc::Receiver<Vec<u8>>,
    to_local: mpsc::Sender<Vec<u8>>,
) -> Result<(), TransportError> {
    let input_to_stream = async {
        while let Some(data) = from_local.recv().await {
            send.write_all(&data)
                .await
                .map_err(|error| TransportError::Codec(error.to_string()))?;
        }
        send.finish()
            .map_err(|error| TransportError::Codec(error.to_string()))?;
        Ok::<(), TransportError>(())
    };

    let stream_to_output = async {
        let mut buf = [0u8; 4096];
        loop {
            let read = recv
                .read(&mut buf)
                .await
                .map_err(|error| TransportError::Codec(error.to_string()))?;
            let Some(read) = read else {
                break;
            };
            if read == 0 {
                break;
            }
            if to_local.send(buf[..read].to_vec()).await.is_err() {
                break;
            }
        }
        Ok::<(), TransportError>(())
    };

    tokio::select! {
        result = input_to_stream => result?,
        result = stream_to_output => result?,
    }
    Ok(())
}
