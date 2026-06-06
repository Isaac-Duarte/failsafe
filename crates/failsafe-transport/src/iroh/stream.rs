use std::sync::Arc;

use failsafe_core::control::PortProtocol;
use failsafe_core::device::DeviceId;
use failsafe_core::message::FeatureMessage;
use iroh::endpoint::{RecvStream, SendStream};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, warn};

use crate::codec;
use crate::port;
use crate::shell;
use crate::transport::TransportError;

const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

pub type ShellAcceptor = mpsc::Sender<ShellSession>;
pub type SharedShellAcceptor = Arc<Mutex<Option<ShellAcceptor>>>;

pub type PortAcceptor = mpsc::Sender<PortSession>;
pub type SharedPortAcceptor = Arc<Mutex<Option<PortAcceptor>>>;

pub struct ShellSession {
    pub from: DeviceId,
    pub rows: u16,
    pub cols: u16,
    pub send: SendStream,
    pub recv: RecvStream,
}

pub struct PortSession {
    pub from: DeviceId,
    pub remote_port: u16,
    pub protocol: PortProtocol,
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
    port_acceptor: SharedPortAcceptor,
    shell_acceptor: SharedShellAcceptor,
) {
    let mut header = [0u8; 4];
    if let Err(error) = read_exact(&mut recv, &mut header).await {
        warn!(%device, "failed to read stream header: {error}");
        return;
    }

    if port::is_port_handshake(&header) {
        let mut tail = [0u8; 3];
        if let Err(error) = read_exact(&mut recv, &mut tail).await {
            warn!(%device, "port stream missing init tail: {error}");
            return;
        }
        let mut init = [0u8; port::PORT_INIT_LEN];
        init[..4].copy_from_slice(&header);
        init[4..].copy_from_slice(&tail);
        let Some((remote_port, protocol)) = port::parse_port_init(&init) else {
            warn!(%device, "port stream has invalid init");
            return;
        };

        let acceptor = port_acceptor.lock().await.clone();
        let Some(acceptor) = acceptor else {
            warn!(%device, "rejected port stream: port acceptor not registered");
            return;
        };

        let session = PortSession {
            from: device,
            remote_port,
            protocol,
            send,
            recv,
        };

        if acceptor.send(session).await.is_err() {
            warn!(%device, "port acceptor closed");
        }
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

pub async fn relay_port_streams<R, W>(
    send: SendStream,
    recv: RecvStream,
    input: R,
    output: W,
) -> Result<(), TransportError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    relay_shell_streams(send, recv, input, output).await
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
