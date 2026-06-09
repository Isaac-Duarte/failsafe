mod control;
mod feature;
mod relay;
mod routing;
mod state;
#[cfg(unix)]
mod tun_fd;
mod tun_helper;
mod tun_iface;
mod tun_setup;

use std::sync::Arc;

use failsafe_transport::iroh::{IrohTransport, LanSession};
use tokio::sync::mpsc;

pub use control::{LanControlBody, LanFeatureControl};
pub use feature::{
    LanFeature, LanFeatureSpec, ID as LAN_FEATURE_ID, update_routing_from_devices,
};
pub use routing::{LanRoutingTable, SharedRoutingTable, shared_routing_table};
pub use state::{LanRuntimeState, SharedLanState, shared_lan_state};
pub use tun_helper::{run_tun_helper, TunHelperError};

pub async fn start_lan_acceptor(iroh: Arc<IrohTransport>) -> mpsc::Receiver<LanSession> {
    let (tx, rx) = mpsc::channel(8);
    iroh.set_lan_acceptor(tx).await;
    rx
}

pub async fn stop_lan_acceptor(iroh: &IrohTransport) {
    iroh.clear_lan_acceptor().await;
}

#[cfg(test)]
mod tests;
