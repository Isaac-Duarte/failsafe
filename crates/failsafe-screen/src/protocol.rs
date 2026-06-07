use iroh::endpoint::{RecvStream, SendStream};
use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("codec error: {0}")]
    Codec(String),
}

async fn read_exact_stream(recv: &mut RecvStream, buf: &mut [u8]) -> Result<(), ProtocolError> {
    let mut offset = 0;
    while offset < buf.len() {
        let read = recv
            .read(&mut buf[offset..])
            .await
            .map_err(|error| ProtocolError::Codec(error.to_string()))?;
        let n = read.unwrap_or(0);
        if n == 0 {
            return Err(ProtocolError::Codec(
                "stream closed before read complete".to_owned(),
            ));
        }
        offset += n;
    }
    Ok(())
}

pub async fn write_nal(send: &mut SendStream, nal: &[u8]) -> Result<(), ProtocolError> {
    let len = u32::try_from(nal.len())
        .map_err(|_| ProtocolError::Codec("nal unit too large".to_owned()))?;
    send.write_all(&len.to_be_bytes())
        .await
        .map_err(|error| ProtocolError::Codec(error.to_string()))?;
    send.write_all(nal)
        .await
        .map_err(|error| ProtocolError::Codec(error.to_string()))?;
    Ok(())
}

pub async fn read_nal(recv: &mut RecvStream) -> Result<Option<Vec<u8>>, ProtocolError> {
    let mut len_buf = [0u8; 4];
    match read_exact_stream(recv, &mut len_buf).await {
        Ok(()) => {}
        Err(ProtocolError::Codec(message)) if message.contains("closed") => return Ok(None),
        Err(error) => return Err(error),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 {
        return Ok(None);
    }
    if len > 16 * 1024 * 1024 {
        return Err(ProtocolError::Codec(
            "nal unit exceeds maximum size".to_owned(),
        ));
    }
    let mut payload = vec![0u8; len];
    read_exact_stream(recv, &mut payload).await?;
    Ok(Some(payload))
}

async fn read_exact_reader<R>(reader: &mut R, buf: &mut [u8]) -> Result<(), std::io::Error>
where
    R: AsyncRead + Unpin,
{
    let mut offset = 0;
    while offset < buf.len() {
        let read = reader.read(&mut buf[offset..]).await?;
        if read == 0 {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
        }
        offset += read;
    }
    Ok(())
}

pub async fn read_nal_from<R>(reader: &mut R) -> Result<Option<Vec<u8>>, std::io::Error>
where
    R: AsyncRead + Unpin,
{
    let mut len_buf = [0u8; 4];
    if let Err(error) = read_exact_reader(reader, &mut len_buf).await {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            return Ok(None);
        }
        return Err(error);
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 {
        return Ok(None);
    }
    if len > 16 * 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "nal unit exceeds maximum size",
        ));
    }
    let mut payload = vec![0u8; len];
    read_exact_reader(reader, &mut payload).await?;
    Ok(Some(payload))
}

#[cfg(test)]
mod tests {
    #[test]
    fn nal_length_encoding_roundtrip() {
        let nal = vec![1, 2, 3, 4, 5];
        let len = u32::try_from(nal.len()).unwrap().to_be_bytes();
        assert_eq!(len, [0, 0, 0, 5]);
    }
}
