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

    // Exit when the PTY closes even if the input channel stays open; the inner
    // to_pty channel bridges async input to the blocking PTY writer.
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

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::time::Duration;

    use tokio::sync::mpsc;

    use super::relay_channels_to_pty;

    struct EofReader;

    impl Read for EofReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Ok(0)
        }
    }

    struct SinkWriter;

    impl Write for SinkWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn relay_exits_when_pty_eof_even_if_input_channel_open() {
        let (input_tx, input_rx) = mpsc::channel(8);
        let (output_tx, _output_rx) = mpsc::channel(8);

        let result = tokio::time::timeout(
            Duration::from_secs(1),
            relay_channels_to_pty(
                input_rx,
                output_tx,
                Box::new(EofReader),
                Box::new(SinkWriter),
            ),
        )
        .await;

        assert!(
            result.is_ok(),
            "relay should not hang waiting for input channel close"
        );
        assert!(result.unwrap().is_ok());

        // Input channel still open; relay should have exited on PTY EOF alone.
        drop(input_tx);
    }
}
