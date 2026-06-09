use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;

use failsafe_core::device::DeviceId;
use failsafe_transport::iroh::{IrohTransport, LanSession, read_lan_packet, write_lan_packet};
use pnet_packet::ipv4::Ipv4Packet;
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, warn};

use crate::routing::SharedRoutingTable;

pub struct LanRelay {
    iroh: Arc<IrohTransport>,
    routing: SharedRoutingTable,
    tun_tx: mpsc::Sender<Vec<u8>>,
    outbound: Arc<Mutex<HashMap<DeviceId, Arc<Mutex<LanSession>>>>>,
}

impl LanRelay {
    pub fn new(
        iroh: Arc<IrohTransport>,
        routing: SharedRoutingTable,
        tun_tx: mpsc::Sender<Vec<u8>>,
    ) -> Self {
        Self {
            iroh,
            routing,
            tun_tx,
            outbound: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn handle_incoming_session(&self, session: LanSession) {
        let routing = self.routing.clone();
        let tun_tx = self.tun_tx.clone();
        let peer = session.from;
        tokio::spawn(async move {
            if let Err(error) = serve_inbound_lan_session(session, routing, tun_tx).await {
                debug!(%peer, "inbound lan session ended: {error}");
            }
        });
    }

    pub async fn run_tun_packet(&self, packet: &[u8]) {
        let Some(dest) = parse_ipv4_dest(packet) else {
            return;
        };

        let peer = {
            let table = self.routing.read().await;
            if table.local_ip() == Some(dest) {
                return;
            }
            table.device_for_ip(dest)
        };

        let Some(peer) = peer else {
            debug!(%dest, "dropping packet with no route");
            return;
        };

        if let Err(error) = self.send_packet(peer, packet).await {
            warn!(%peer, %dest, "failed to forward lan packet: {error}");
        }
    }

    async fn send_packet(&self, peer: DeviceId, packet: &[u8]) -> Result<(), String> {
        let session = self.get_or_open_session(peer).await?;
        let mut guard = session.lock().await;
        write_lan_packet(&mut guard.send, packet)
            .await
            .map_err(|error| error.to_string())
    }

    async fn get_or_open_session(
        &self,
        peer: DeviceId,
    ) -> Result<Arc<Mutex<LanSession>>, String> {
        {
            let sessions = self.outbound.lock().await;
            if let Some(session) = sessions.get(&peer) {
                return Ok(session.clone());
            }
        }

        let session = self
            .iroh
            .open_lan_stream(peer)
            .await
            .map_err(|error| error.to_string())?;
        let shared = Arc::new(Mutex::new(session));
        self.outbound
            .lock()
            .await
            .insert(peer, shared.clone());
        Ok(shared)
    }
}

async fn serve_inbound_lan_session(
    mut session: LanSession,
    routing: SharedRoutingTable,
    tun_tx: mpsc::Sender<Vec<u8>>,
) -> Result<(), String> {
    let peer = session.from;
    loop {
        let packet = read_lan_packet(&mut session.recv)
            .await
            .map_err(|error| error.to_string())?;

        let Some(source) = parse_ipv4_source(&packet) else {
            continue;
        };

        let expected = routing.read().await.expected_source_ip(peer);
        if expected != Some(source) {
            warn!(
                %peer,
                %source,
                ?expected,
                "dropping lan packet with unexpected source IP"
            );
            continue;
        }

        tun_tx
            .send(packet)
            .await
            .map_err(|_| "tun writer closed".to_owned())?;
    }
}

pub fn parse_ipv4_dest(packet: &[u8]) -> Option<Ipv4Addr> {
    let ip = Ipv4Packet::new(packet)?;
    Some(ip.get_destination())
}

pub fn parse_ipv4_source(packet: &[u8]) -> Option<Ipv4Addr> {
    let ip = Ipv4Packet::new(packet)?;
    Some(ip.get_source())
}
