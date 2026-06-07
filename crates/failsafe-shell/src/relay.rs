use std::io::{Read, Write};
use std::sync::Arc;

use failsafe_transport::iroh::{
    IrohTransport, ShellSession, relay_shell_streams, relay_shell_to_channels,
};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::host::run_shell_host;

/// Relay bytes between async channels and a blocking PTY pair.
pub async fn relay_channels_to_pty(
    mut from_stream: mpsc::Receiver<Vec<u8>>,
    to_stream: mpsc::Sender<Vec<u8>>,
    pty_reader: Box<dyn Read + Send>,
    pty_writer: Box<dyn Write + Send>,
) -> Result<(), std::io::Error> {
    let (to_pty_tx, mut to_pty_rx) = mpsc::channel::<Vec<u8>>(64);

    let mut read_task = tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
        let mut reader = pty_reader;
        let mut buf = [0u8; 4096];
        loop {
            let read = reader.read(&mut buf)?;
            if read == 0 {
                break;
            }
            if to_stream.blocking_send(buf[..read].to_vec()).is_err() {
                break;
            }
        }
        Ok(())
    });

    let write_task = tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
        let mut writer = pty_writer;
        while let Some(data) = to_pty_rx.blocking_recv() {
            writer.write_all(&data)?;
            writer.flush()?;
        }
        Ok(())
    });

    let mut stream_open = true;
    loop {
        tokio::select! {
            read_result = &mut read_task, if !read_task.is_finished() => {
                drop(to_pty_tx);
                read_result??;
                break;
            }
            data = from_stream.recv(), if stream_open => {
                match data {
                    Some(data) => {
                        if to_pty_tx.send(data).await.is_err() {
                            stream_open = false;
                        }
                    }
                    None => stream_open = false,
                }
            }
            else => break,
        }
    }

    let read_result = if read_task.is_finished() {
        Ok(())
    } else {
        read_task.await.unwrap_or_else(|error| {
            Err(std::io::Error::other(format!(
                "pty read task failed: {error}"
            )))
        })
    };
    let write_result = write_task.await.unwrap_or_else(|error| {
        Err(std::io::Error::other(format!(
            "pty write task failed: {error}"
        )))
    });

    read_result?;
    write_result?;
    Ok(())
}

pub async fn start_shell_acceptor(iroh: Arc<IrohTransport>) -> mpsc::Receiver<ShellSession> {
    let (tx, rx) = mpsc::channel(8);
    iroh.set_shell_acceptor(tx).await;
    rx
}

pub async fn stop_shell_acceptor(iroh: &IrohTransport) {
    iroh.clear_shell_acceptor().await;
}

pub async fn handle_incoming_shell(session: ShellSession) {
    let device = session.from;
    debug!(%device, "accepted shell session");

    let (to_pty_tx, to_pty_rx) = mpsc::channel::<Vec<u8>>(64);
    let (from_pty_tx, from_pty_rx) = mpsc::channel::<Vec<u8>>(64);

    let host = tokio::spawn(async move {
        if let Err(error) = run_shell_host(session.rows, session.cols, to_pty_rx, from_pty_tx).await
        {
            warn!(%device, "shell host exited with error: {error}");
        }
    });

    let relay = tokio::spawn(async move {
        if let Err(error) =
            relay_shell_to_channels(session.send, session.recv, from_pty_rx, to_pty_tx).await
        {
            warn!(%device, "shell relay exited with error: {error}");
        }
    });

    let _ = tokio::join!(host, relay);
}

pub async fn run_outgoing_shell(
    iroh: &IrohTransport,
    session: ShellSession,
    input: impl tokio::io::AsyncRead + Unpin,
    output: impl tokio::io::AsyncWrite + Unpin,
) -> Result<(), failsafe_transport::transport::TransportError> {
    let _ = iroh;
    relay_shell_streams(session.send, session.recv, input, output).await
}
