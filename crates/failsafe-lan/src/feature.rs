use std::sync::Arc;

use async_trait::async_trait;
use failsafe_core::feature::{Feature, FeatureError, FeatureId, FeatureSpec};
use failsafe_core::message::FeatureMessage;
use failsafe_core::virtual_lan::parse_virtual_ip;
use failsafe_transport::iroh::IrohTransport;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::warn;

use crate::relay::LanRelay;
use crate::routing::SharedRoutingTable;
use crate::state::SharedLanState;
use crate::tun_iface::{TunError, TunHandle};
use crate::{start_lan_acceptor, stop_lan_acceptor};

pub const ID: &str = "virtual_lan";

pub struct LanFeatureSpec;

impl FeatureSpec for LanFeatureSpec {
    fn id() -> &'static str {
        ID
    }

    fn label() -> &'static str {
        "Virtual LAN"
    }

    fn description() -> &'static str {
        "Share a private virtual network with paired devices for LAN gaming"
    }
}

pub struct LanFeature {
    iroh: Arc<IrohTransport>,
    routing: SharedRoutingTable,
    runtime: SharedLanState,
    host_task: Option<JoinHandle<()>>,
    tun_task: Option<JoinHandle<()>>,
}

impl LanFeature {
    pub fn new(
        iroh: Arc<IrohTransport>,
        routing: SharedRoutingTable,
        runtime: SharedLanState,
    ) -> Self {
        Self {
            iroh,
            routing,
            runtime,
            host_task: None,
            tun_task: None,
        }
    }
}

#[async_trait]
impl Feature for LanFeature {
    fn id(&self) -> FeatureId {
        LanFeatureSpec::feature_id()
    }

    async fn start(&mut self) -> Result<(), FeatureError> {
        if self.host_task.is_some() {
            return Ok(());
        }

        let local_ip = {
            let table = self.routing.read().await;
            table.local_ip()
        };

        let Some(local_ip) = local_ip else {
            let message = "virtual IP not assigned yet; wait for server sync".to_owned();
            *self.runtime.write().await = crate::state::LanRuntimeState::from_error(message.clone());
            return Err(FeatureError::Failed(
                LanFeatureSpec::feature_id(),
                message,
            ));
        };

        let tun = match TunHandle::open(local_ip) {
            Ok(tun) => tun,
            Err(TunError::PermissionDenied(message)) => {
                let hint = format!(
                    "{message}. Run the daemon as administrator/root to use virtual LAN."
                );
                *self.runtime.write().await =
                    crate::state::LanRuntimeState::from_error(hint.clone());
                return Err(FeatureError::Failed(LanFeatureSpec::feature_id(), hint));
            }
            Err(error) => {
                let message = error.to_string();
                *self.runtime.write().await =
                    crate::state::LanRuntimeState::from_error(message.clone());
                return Err(FeatureError::Failed(
                    LanFeatureSpec::feature_id(),
                    message,
                ));
            }
        };

        *self.runtime.write().await = crate::state::LanRuntimeState::from_interface(
            local_ip,
            tun.subnet_cidr(),
        );

        let (tun_tx, mut tun_rx) = mpsc::channel(256);
        let relay = Arc::new(LanRelay::new(
            self.iroh.clone(),
            self.routing.clone(),
            tun_tx,
        ));

        let device = tun.device();
        let relay_for_tun = relay.clone();
        self.tun_task = Some(tokio::spawn(async move {
            let _tun = tun;
            let mut buf = vec![0u8; 65535];
            loop {
                tokio::select! {
                    read_result = async {
                        let mut guard = device.lock().await;
                        buf.resize(65535, 0);
                        let read = tokio::io::AsyncReadExt::read(&mut *guard, &mut buf).await;
                        (read, guard)
                    } => {
                        let (read, _guard) = read_result;
                        match read {
                            Ok(0) => break,
                            Ok(n) => {
                                relay_for_tun.run_tun_packet(&buf[..n]).await;
                            }
                            Err(error) => {
                                warn!("tun read failed: {error}");
                                break;
                            }
                        }
                    }
                    packet = tun_rx.recv() => {
                        match packet {
                            Some(packet) => {
                                let mut guard = device.lock().await;
                                if let Err(error) = tokio::io::AsyncWriteExt::write_all(&mut *guard, &packet).await {
                                    warn!("failed to write packet to tun: {error}");
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
        }));

        let mut sessions = start_lan_acceptor(self.iroh.clone()).await;
        let relay_for_sessions = relay.clone();
        self.host_task = Some(tokio::spawn(async move {
            while let Some(session) = sessions.recv().await {
                relay_for_sessions.handle_incoming_session(session).await;
            }
        }));

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), FeatureError> {
        if let Some(task) = self.host_task.take() {
            task.abort();
        }
        if let Some(task) = self.tun_task.take() {
            task.abort();
        }
        stop_lan_acceptor(&self.iroh).await;
        *self.runtime.write().await = crate::state::LanRuntimeState::default();
        Ok(())
    }

    async fn handle_message(&mut self, _message: FeatureMessage) -> Result<(), FeatureError> {
        Ok(())
    }
}

pub async fn update_routing_from_devices(
    routing: &SharedRoutingTable,
    self_id: failsafe_core::device::DeviceId,
    devices: &[failsafe_core::api::DeviceInfo],
) {
    let mut table = routing.write().await;

    if let Some(self_device) = devices.iter().find(|d| d.device_id == self_id) {
        table.set_local_ip(
            self_device
                .virtual_ip
                .as_deref()
                .and_then(parse_virtual_ip),
        );
    }

    let peers = devices
        .iter()
        .filter(|d| d.device_id != self_id)
        .filter(|d| {
            d.enabled_features
                .iter()
                .any(|f| f.as_str() == LanFeatureSpec::id())
        })
        .filter_map(|d| d.virtual_ip.clone().map(|ip| (d.device_id, ip)))
        .collect::<Vec<_>>();

    table.replace_peers(peers);
}
