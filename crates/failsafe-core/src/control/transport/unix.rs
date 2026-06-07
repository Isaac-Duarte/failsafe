use std::path::{Path, PathBuf};

use tokio::net::{UnixListener, UnixStream};

use super::super::ControlError;

pub type ControlStream = UnixStream;

pub struct ControlListener(UnixListener);

impl ControlListener {
    pub async fn accept(&self) -> Result<(ControlStream, ()), ControlError> {
        self.0
            .accept()
            .await
            .map(|(stream, _)| (stream, ()))
            .map_err(ControlError::Io)
    }
}

pub fn endpoint_path() -> Result<PathBuf, ControlError> {
    let base = dirs::runtime_dir()
        .or_else(dirs::config_dir)
        .ok_or_else(|| {
            ControlError::Config("could not determine control socket directory".to_owned())
        })?;
    Ok(base.join("failsafe").join("control.sock"))
}

pub async fn bind_control(path: &Path) -> Result<ControlListener, ControlError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    remove_stale_control_endpoint(path).await?;
    let listener = UnixListener::bind(path).map(ControlListener).map_err(ControlError::Io)?;
    restrict_socket_permissions(path)?;
    Ok(listener)
}

fn restrict_socket_permissions(path: &Path) -> Result<(), ControlError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub async fn connect_control(path: &Path) -> Result<ControlStream, ControlError> {
    UnixStream::connect(path).await.map_err(ControlError::Io)
}

pub async fn remove_stale_control_endpoint(path: &Path) -> Result<(), ControlError> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
