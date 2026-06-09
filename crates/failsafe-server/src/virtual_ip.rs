use std::net::Ipv4Addr;

use failsafe_core::virtual_lan::assign_virtual_ip;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::entity::{Device, device};
use crate::error::{ServerError, ServerResult};

pub(crate) async fn ensure_virtual_ip<C>(
    conn: &C,
    account_id: Uuid,
    device_id: Uuid,
    existing: Option<String>,
) -> ServerResult<String>
where
    C: ConnectionTrait,
{
    if let Some(ip) = existing {
        if ip.parse::<Ipv4Addr>().is_ok() {
            return Ok(ip);
        }
    }

    let taken: Vec<String> = Device::find()
        .filter(device::Column::AccountId.eq(account_id))
        .filter(device::Column::DeletedAt.is_null())
        .all(conn)
        .await?
        .into_iter()
        .filter_map(|model| {
            if model.device_id == device_id {
                return None;
            }
            model.virtual_ip
        })
        .collect();

    let mut candidate = assign_virtual_ip(account_id, device_id);
    let mut salt = 0u8;
    while taken.contains(&candidate.to_string()) {
        salt = salt.wrapping_add(1);
        let octets = candidate.octets();
        let host = 2 + ((octets[3] as u16 + salt as u16) % 253);
        candidate = Ipv4Addr::new(octets[0], octets[1], octets[2], host as u8);
        if salt == 255 {
            return Err(ServerError::Internal(
                "failed to allocate unique virtual IP for account".to_owned(),
            ));
        }
    }

    Ok(candidate.to_string())
}
