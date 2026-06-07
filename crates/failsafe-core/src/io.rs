use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Relays bytes between two async read/write pairs until either side closes.
pub async fn relay_bidirectional<R1, W1, R2, W2, E>(
    mut read_a: R1,
    mut write_a: W1,
    read_a_buf: usize,
    mut read_b: R2,
    mut write_b: W2,
    read_b_buf: usize,
    map_io_error: impl Fn(std::io::Error) -> E,
    shutdown_a_write: bool,
) -> Result<(), E>
where
    R1: AsyncRead + Unpin,
    W1: AsyncWrite + Unpin,
    R2: AsyncRead + Unpin,
    W2: AsyncWrite + Unpin,
{
    let mut buf_a = vec![0u8; read_a_buf];
    let mut buf_b = vec![0u8; read_b_buf];

    loop {
        tokio::select! {
            read = read_a.read(&mut buf_a) => {
                let read = read.map_err(&map_io_error)?;
                if read == 0 {
                    break;
                }
                write_b
                    .write_all(&buf_a[..read])
                    .await
                    .map_err(&map_io_error)?;
                write_b.flush().await.map_err(&map_io_error)?;
            }
            read = read_b.read(&mut buf_b) => {
                let read = read.map_err(&map_io_error)?;
                if read == 0 {
                    break;
                }
                write_a
                    .write_all(&buf_b[..read])
                    .await
                    .map_err(&map_io_error)?;
                write_a.flush().await.map_err(&map_io_error)?;
            }
        }
    }

    if shutdown_a_write {
        let _ = write_a.shutdown().await;
    }

    Ok(())
}
