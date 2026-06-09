use std::collections::HashMap;
use std::net::Ipv4Addr;

use failsafe_core::device::DeviceId;
use failsafe_core::virtual_lan::parse_virtual_ip;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Default)]
pub struct LanRoutingTable {
    local_ip: Option<Ipv4Addr>,
    ip_to_device: HashMap<Ipv4Addr, DeviceId>,
    device_to_ip: HashMap<DeviceId, Ipv4Addr>,
}

impl LanRoutingTable {
    pub fn set_local_ip(&mut self, ip: Option<Ipv4Addr>) {
        self.local_ip = ip;
    }

    pub fn local_ip(&self) -> Option<Ipv4Addr> {
        self.local_ip
    }

    pub fn replace_peers(&mut self, peers: impl IntoIterator<Item = (DeviceId, String)>) {
        self.ip_to_device.clear();
        self.device_to_ip.clear();
        for (device, ip_str) in peers {
            if let Some(ip) = parse_virtual_ip(&ip_str) {
                self.ip_to_device.insert(ip, device);
                self.device_to_ip.insert(device, ip);
            }
        }
    }

    pub fn device_for_ip(&self, ip: Ipv4Addr) -> Option<DeviceId> {
        self.ip_to_device.get(&ip).copied()
    }

    pub fn ip_for_device(&self, device: DeviceId) -> Option<Ipv4Addr> {
        self.device_to_ip.get(&device).copied()
    }

    pub fn expected_source_ip(&self, device: DeviceId) -> Option<Ipv4Addr> {
        self.ip_for_device(device)
    }

    pub fn peers(&self) -> Vec<(DeviceId, Ipv4Addr)> {
        self.device_to_ip
            .iter()
            .map(|(device, ip)| (*device, *ip))
            .collect()
    }
}

pub type SharedRoutingTable = std::sync::Arc<RwLock<LanRoutingTable>>;

pub fn shared_routing_table() -> SharedRoutingTable {
    std::sync::Arc::new(RwLock::new(LanRoutingTable::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_by_virtual_ip() {
        let device = DeviceId::new();
        let mut table = LanRoutingTable::default();
        table.replace_peers([(device, "100.64.1.5".to_owned())]);
        assert_eq!(
            table.device_for_ip(Ipv4Addr::new(100, 64, 1, 5)),
            Some(device)
        );
    }
}
