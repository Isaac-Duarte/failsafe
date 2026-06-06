use tokio::io::{AsyncRead, AsyncWrite};
use tracing::warn;

use crate::protocol::{
    PACKET_TAG_CONTROL, PACKET_TAG_FRAME, ProtocolError, read_tagged_packet,
    write_tagged_packet, write_tagged_packet_with_flush,
};

pub async fn relay_tagged_bidirectional<Ri, Wi, Ro, Wo>(
    mut inbound_read: Ri,
    mut outbound_write: Wo,
    mut outbound_read: Ro,
    mut inbound_write: Wi,
) -> Result<(), ProtocolError>
where
    Ri: AsyncRead + Unpin,
    Wi: AsyncWrite + Unpin,
    Ro: AsyncRead + Unpin,
    Wo: AsyncWrite + Unpin,
{
    let inbound_to_outbound = async {
        loop {
            match read_tagged_packet(&mut inbound_read).await {
                Ok((tag, payload)) => {
                    let flush = tag != PACKET_TAG_FRAME;
                    write_tagged_packet_with_flush(&mut outbound_write, tag, &payload, flush)
                        .await?;
                }
                Err(ProtocolError::Io(error))
                    if error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        Ok::<(), ProtocolError>(())
    };

    let outbound_to_inbound = async {
        loop {
            match read_tagged_packet(&mut outbound_read).await {
                Ok((PACKET_TAG_CONTROL, payload)) => {
                    write_tagged_packet(&mut inbound_write, PACKET_TAG_CONTROL, &payload).await?;
                }
                Ok((tag, _)) => warn!("ignoring unexpected outbound screen packet tag: {tag}"),
                Err(ProtocolError::Io(error))
                    if error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        Ok::<(), ProtocolError>(())
    };

    tokio::select! {
        result = inbound_to_outbound => result,
        result = outbound_to_inbound => result,
    }
}
