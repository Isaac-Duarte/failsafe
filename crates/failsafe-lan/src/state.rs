use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LanRuntimeState {
    pub virtual_ip: Option<String>,
    pub subnet_cidr: Option<String>,
    pub interface_up: bool,
    pub message: Option<String>,
}

impl LanRuntimeState {
    pub fn from_interface(local_ip: Ipv4Addr, subnet_cidr: String) -> Self {
        Self {
            virtual_ip: Some(local_ip.to_string()),
            subnet_cidr: Some(subnet_cidr),
            interface_up: true,
            message: None,
        }
    }

    pub fn from_error(message: String) -> Self {
        Self {
            virtual_ip: None,
            subnet_cidr: None,
            interface_up: false,
            message: Some(message),
        }
    }
}

pub type SharedLanState = std::sync::Arc<tokio::sync::RwLock<LanRuntimeState>>;

pub fn shared_lan_state() -> SharedLanState {
    std::sync::Arc::new(tokio::sync::RwLock::new(LanRuntimeState::default()))
}
