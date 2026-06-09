use std::sync::Arc;

use failsafe_transport::iroh::{
    DesktopSession, IrohTransport, InputSession,
};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::host::run_desktop_host;
use crate::input::run_input_host;
use crate::viewer::run_desktop_viewer_with_input;

pub async fn start_desktop_acceptor(iroh: Arc<IrohTransport>) -> mpsc::Receiver<DesktopSession> {
    let (tx, rx) = mpsc::channel(4);
    iroh.set_desktop_acceptor(tx).await;
    rx
}

pub async fn stop_desktop_acceptor(iroh: &IrohTransport) {
    iroh.clear_desktop_acceptor().await;
}

pub async fn start_input_acceptor(iroh: Arc<IrohTransport>) -> mpsc::Receiver<InputSession> {
    let (tx, rx) = mpsc::channel(4);
    iroh.set_input_acceptor(tx).await;
    rx
}

pub async fn stop_input_acceptor(iroh: &IrohTransport) {
    iroh.clear_input_acceptor().await;
}

pub async fn handle_incoming_desktop(session: DesktopSession) {
    let device = session.from;
    debug!(%device, "accepted desktop session");
    run_desktop_host(session).await;
}

pub async fn handle_incoming_input(session: InputSession) {
    run_input_host(session).await;
}

pub async fn run_outgoing_desktop(
    iroh: &IrohTransport,
    session: DesktopSession,
) -> Result<(), failsafe_transport::transport::TransportError> {
    let input = if session.view_only {
        None
    } else {
        Some(iroh.open_input_stream(session.from).await?)
    };

    match input {
        None => {
            crate::viewer::run_desktop_viewer_view_only(session).await?;
        }
        Some(mut input_session) => {
            tokio::spawn(async move {
                let mut buf = [0u8; 64];
                loop {
                    match input_session.recv.read(&mut buf).await {
                        Ok(Some(0)) | Ok(None) | Err(_) => break,
                        Ok(Some(_)) => {}
                    }
                }
            });
            run_desktop_viewer_with_input(session, input_session.send).await?;
        }
    }

    Ok(())
}

pub fn spawn_acceptor_loops(
    iroh: Arc<IrohTransport>,
    mut desktop_sessions: mpsc::Receiver<DesktopSession>,
    mut input_sessions: mpsc::Receiver<InputSession>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                session = desktop_sessions.recv() => {
                    match session {
                        Some(session) => {
                            tokio::spawn(handle_incoming_desktop(session));
                        }
                        None => break,
                    }
                }
                session = input_sessions.recv() => {
                    match session {
                        Some(session) => {
                            tokio::spawn(handle_incoming_input(session));
                        }
                        None => break,
                    }
                }
            }
        }
        stop_desktop_acceptor(&iroh).await;
        stop_input_acceptor(&iroh).await;
        warn!("desktop acceptor loop exited");
    })
}
