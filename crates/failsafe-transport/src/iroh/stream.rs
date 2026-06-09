use std::sync::Arc;

use failsafe_core::control::PortProtocol;
use failsafe_core::device::DeviceId;
use failsafe_core::message::FeatureMessage;
use iroh::endpoint::{RecvStream, SendStream};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, warn};

use crate::codec;
use crate::lan;
use crate::port;
use crate::shell;
use crate::transport::TransportError;

const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

pub type ShellAcceptor = mpsc::Sender<ShellSession>;
pub type SharedShellAcceptor = Arc<Mutex<Option<ShellAcceptor>>>;

pub type PortAcceptor = mpsc::Sender<PortSession>;
pub type SharedPortAcceptor = Arc<Mutex<Option<PortAcceptor>>>;

pub type LanAcceptor = mpsc::Sender<LanSession>;
pub type SharedLanAcceptor = Arc<Mutex<Option<LanAcceptor>>>;

pub struct LanSession {
    pub from: DeviceId,
    pub send: SendStream,
    pub recv: RecvStream,
}

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
        let n = read.unwrap_or(0);
        if n == 0 {
            return Err(TransportError::Codec(
                "stream closed before read complete".to_owned(),
            ));
        }
        offset += n;
    }
    Ok(())
}

pub async fn handle_incoming_bi_stream(
    send: SendStream,
    mut recv: RecvStream,
    device: DeviceId,
    local_device_id: DeviceId,
    inbox: mpsc::Sender<FeatureMessage>,
    port_acceptor: SharedPortAcceptor,
    shell_acceptor: SharedShellAcceptor,
    lan_acceptor: SharedLanAcceptor,
) {
    let mut header = [0u8; 4];
    if let Err(error) = read_exact(&mut recv, &mut header).await {
        warn!(%device, "failed to read stream header: {error}");
        return;
    }

    if port::is_port_handshake(&header) {
        try_accept_port_stream(header, recv, send, port_acceptor, device).await;
        return;
    }

    if lan::is_lan_handshake(&header) {
        try_accept_lan_stream(recv, send, lan_acceptor, device).await;
        return;
    }

    if shell::is_shell_handshake(&header) {
        try_accept_shell_stream(header, recv, send, shell_acceptor, device).await;
        return;
    }

    handle_feature_frame(header, recv, send, device, local_device_id, inbox).await;
}

async fn try_accept_lan_stream(
    recv: RecvStream,
    send: SendStream,
    lan_acceptor: SharedLanAcceptor,
    device: DeviceId,
) {
    let session = LanSession {
        from: device,
        send,
        recv,
    };
    forward_to_acceptor(
        lan_acceptor,
        device,
        session,
        "rejected lan stream: lan acceptor not registered",
        "lan acceptor closed",
    )
    .await;
}

async fn try_accept_port_stream(
    header: [u8; 4],
    mut recv: RecvStream,
    send: SendStream,
    port_acceptor: SharedPortAcceptor,
    device: DeviceId,
) {
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

    let session = PortSession {
        from: device,
        remote_port,
        protocol,
        send,
        recv,
    };
    forward_to_acceptor(
        port_acceptor,
        device,
        session,
        "rejected port stream: port acceptor not registered",
        "port acceptor closed",
    )
    .await;
}

async fn try_accept_shell_stream(
    header: [u8; 4],
    mut recv: RecvStream,
    send: SendStream,
    shell_acceptor: SharedShellAcceptor,
    device: DeviceId,
) {
    let _ = header;
    let mut size_buf = [0u8; 4];
    if let Err(error) = read_exact(&mut recv, &mut size_buf).await {
        warn!(%device, "shell stream missing terminal size: {error}");
        return;
    }
    let rows = u16::from_be_bytes(size_buf[..2].try_into().expect("rows"));
    let cols = u16::from_be_bytes(size_buf[2..].try_into().expect("cols"));

    let session = ShellSession {
        from: device,
        rows,
        cols,
        send,
        recv,
    };
    forward_to_acceptor(
        shell_acceptor,
        device,
        session,
        "rejected shell stream: shell acceptor not registered",
        "shell acceptor closed",
    )
    .await;
}

async fn forward_to_acceptor<T>(
    acceptor: Arc<Mutex<Option<mpsc::Sender<T>>>>,
    device: DeviceId,
    session: T,
    not_registered_msg: &'static str,
    closed_msg: &'static str,
) {
    let acceptor = acceptor.lock().await.clone();
    let Some(acceptor) = acceptor else {
        warn!(%device, "{not_registered_msg}");
        return;
    };

    if acceptor.send(session).await.is_err() {
        warn!(%device, "{closed_msg}");
    }
}

async fn handle_feature_frame(
    header: [u8; 4],
    mut recv: RecvStream,
    send: SendStream,
    device: DeviceId,
    local_device_id: DeviceId,
    inbox: mpsc::Sender<FeatureMessage>,
) {
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
        Ok(mut message) => {
            if message.to != local_device_id {
                warn!(
                    %device,
                    claimed_from = %message.from,
                    claimed_to = %message.to,
                    %local_device_id,
                    "rejected inbound frame with mismatched recipient"
                );
                return;
            }
            message.from = device;
            if inbox.send(message).await.is_err() {
                debug!("inbox closed while delivering message");
            }
        }
        Err(error) => warn!(%device, "failed to decode inbound frame: {error}"),
    }

    // Drain the receive half so the remote peer's `finish()` cannot block on flow control.
    tokio::spawn(async move {
        drop(send);
        let mut buf = [0u8; 1024];
        loop {
            match recv.read(&mut buf).await {
                Ok(Some(0)) | Ok(None) => break,
                Ok(Some(_)) => {}
                Err(_) => break,
            }
        }
    });
}

pub async fn read_lan_packet(recv: &mut RecvStream) -> Result<Vec<u8>, TransportError> {
    let mut len_buf = [0u8; 4];
    read_exact(recv, &mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 || len > lan::MAX_LAN_PACKET_SIZE {
        return Err(TransportError::Codec(format!(
            "invalid lan packet length: {len}"
        )));
    }
    let mut packet = vec![0u8; len];
    read_exact(recv, &mut packet).await?;
    Ok(packet)
}

pub async fn write_lan_packet(send: &mut SendStream, packet: &[u8]) -> Result<(), TransportError> {
    if packet.is_empty() || packet.len() > lan::MAX_LAN_PACKET_SIZE {
        return Err(TransportError::Codec(format!(
            "invalid lan packet size: {}",
            packet.len()
        )));
    }
    let len = u32::try_from(packet.len())
        .map_err(|_| TransportError::Codec("lan packet too large".to_owned()))?;
    send.write_all(&len.to_be_bytes())
        .await
        .map_err(|error| TransportError::Codec(error.to_string()))?;
    send.write_all(packet)
        .await
        .map_err(|error| TransportError::Codec(error.to_string()))?;
    Ok(())
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
