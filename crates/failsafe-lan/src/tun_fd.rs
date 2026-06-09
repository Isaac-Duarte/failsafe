//! Pass a TUN file descriptor between processes over a Unix socket (SCM_RIGHTS).

#[cfg(unix)]
mod unix {
    use std::io::{IoSlice, IoSliceMut};
    use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
    use std::os::unix::net::UnixStream;

    use nix::sys::socket::{ControlMessage, ControlMessageOwned, MsgFlags, recvmsg, sendmsg};

    #[derive(Debug, thiserror::Error)]
    pub enum FdPassError {
        #[error("fd pass io error: {0}")]
        Io(#[from] std::io::Error),

        #[error("fd pass nix error: {0}")]
        Nix(#[from] nix::Error),

        #[error("fd pass protocol error: {0}")]
        Protocol(String),
    }

    pub fn send_tun_fd(stream: &UnixStream, interface_name: &str, fd: RawFd) -> Result<(), FdPassError> {
        let name = interface_name.as_bytes();
        if name.is_empty() || name.len() > 255 {
            return Err(FdPassError::Protocol(
                "interface name must be 1-255 bytes".to_owned(),
            ));
        }

        let iov = [IoSlice::new(name)];
        sendmsg(
            stream.as_raw_fd(),
            &iov,
            &[ControlMessage::ScmRights(&[fd])],
            MsgFlags::empty(),
        )?;
        Ok(())
    }

    pub fn recv_tun_fd(stream: &UnixStream) -> Result<(String, OwnedFd), FdPassError> {
        let mut name_buf = [0u8; 256];
        let mut iov = [IoSliceMut::new(&mut name_buf)];
        let mut cmsg = nix::cmsg_space!(RawFd);

        let msg = recvmsg::<()>(
            stream.as_raw_fd(),
            &mut iov,
            Some(&mut cmsg),
            MsgFlags::empty(),
        )?;

        let received = msg.bytes;
        if received == 0 {
            return Err(FdPassError::Protocol(
                "helper closed socket before sending fd".to_owned(),
            ));
        }

        let name = std::str::from_utf8(&name_buf[..received])
            .map_err(|error| FdPassError::Protocol(error.to_string()))?
            .to_owned();

        let mut fds = Vec::new();
        for cmsg in msg.cmsgs().map_err(FdPassError::Nix)? {
            if let ControlMessageOwned::ScmRights(rights) = cmsg {
                fds.extend(rights);
            }
        }

        if fds.len() != 1 {
            return Err(FdPassError::Protocol(format!(
                "expected one fd from helper, got {}",
                fds.len()
            )));
        }

        let fd = fds[0];
        // SAFETY: fd was received from a trusted helper over SCM_RIGHTS.
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        Ok((name, owned))
    }

    pub fn owned_fd_raw(fd: OwnedFd) -> RawFd {
        fd.as_raw_fd()
    }

    pub fn into_raw_fd(fd: OwnedFd) -> RawFd {
        fd.into_raw_fd()
    }
}

#[cfg(unix)]
pub use unix::*;
