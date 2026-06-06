use std::io::{Read, Write};

use tokio::sync::mpsc;

/// Relay bytes between async channels and a blocking PTY pair.
pub async fn relay_channels_to_pty(
    mut from_stream: mpsc::Receiver<Vec<u8>>,
    to_stream: mpsc::Sender<Vec<u8>>,
    pty_reader: Box<dyn Read + Send>,
    pty_writer: Box<dyn Write + Send>,
) -> Result<(), std::io::Error> {
    let (to_pty_tx, mut to_pty_rx) = mpsc::channel::<Vec<u8>>(64);

    let read_task = tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
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

    let stream_result = async {
        while let Some(data) = from_stream.recv().await {
            if to_pty_tx.send(data).await.is_err() {
                break;
            }
        }
        Ok::<(), std::io::Error>(())
    }
    .await;

    let read_result = read_task.await.unwrap_or_else(|error| {
        Err(std::io::Error::other(format!("pty read task failed: {error}")))
    });
    let write_result = write_task.await.unwrap_or_else(|error| {
        Err(std::io::Error::other(format!("pty write task failed: {error}")))
    });

    stream_result?;
    read_result?;
    write_result?;
    Ok(())
}
