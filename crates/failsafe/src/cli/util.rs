use std::str::FromStr;

use failsafe::DaemonError;
use failsafe_core::api::DeviceInfo;
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;

pub fn parse_device_id(id: &str) -> Result<DeviceId, DaemonError> {
    DeviceId::from_str(id.trim())
        .map_err(|error| DaemonError::Config(format!("invalid device id `{id}`: {error}")))
}

pub fn parse_feature_list(features: &str) -> Result<Vec<FeatureId>, DaemonError> {
    if features.trim().is_empty() {
        return Ok(vec![]);
    }

    features
        .split(',')
        .map(|part| {
            FeatureId::from_str(part.trim()).map_err(|error| {
                DaemonError::Config(format!("unknown feature `{}`: {error}", part.trim()))
            })
        })
        .collect()
}

pub fn default_hostname() -> String {
    gethostname::gethostname().to_string_lossy().into_owned()
}

pub fn resolve_device_target(
    query: &str,
    self_id: DeviceId,
    devices: &[DeviceInfo],
) -> Result<DeviceInfo, DaemonError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(DaemonError::Config(
            "device name cannot be empty".to_owned(),
        ));
    }

    if let Ok(device_id) = DeviceId::from_str(query) {
        let device = devices
            .iter()
            .find(|device| device.device_id == device_id)
            .cloned()
            .ok_or_else(|| DaemonError::Config(format!("device `{query}` not found")))?;
        if device.device_id == self_id {
            return Err(DaemonError::Config(
                "cannot open a shell to this device".to_owned(),
            ));
        }
        return Ok(device);
    }

    let query_lower = query.to_ascii_lowercase();
    let matches: Vec<_> = devices
        .iter()
        .filter(|device| {
            device.device_id != self_id && device.name.to_ascii_lowercase() == query_lower
        })
        .collect();

    match matches.len() {
        0 => Err(DaemonError::Config(format!("device `{query}` not found"))),
        1 => Ok(matches[0].clone()),
        _ => {
            let names = matches
                .iter()
                .map(|device| format!("{} ({})", device.name, device.device_id))
                .collect::<Vec<_>>()
                .join(", ");
            Err(DaemonError::Config(format!(
                "ambiguous device name `{query}`; matches: {names}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_feature_list_splits_comma_separated_values() {
        let features = parse_feature_list("clipboard").unwrap();
        assert_eq!(features, vec![FeatureId::Clipboard]);
    }

    #[test]
    fn parse_feature_list_accepts_empty_string() {
        assert!(parse_feature_list("").unwrap().is_empty());
        assert!(parse_feature_list("  ").unwrap().is_empty());
    }

    #[test]
    fn parse_device_id_rejects_invalid_uuid() {
        assert!(parse_device_id("not-a-uuid").is_err());
    }

    #[test]
    fn resolve_device_target_by_name() {
        let self_id = DeviceId::new();
        let target_id = DeviceId::new();
        let devices = vec![
            DeviceInfo {
                device_id: self_id,
                name: "local".to_owned(),
                iroh_public_key: "key".to_owned(),
                enabled_features: vec![],
                last_seen: None,
                online: true,
            },
            DeviceInfo {
                device_id: target_id,
                name: "laptop".to_owned(),
                iroh_public_key: "key2".to_owned(),
                enabled_features: vec![],
                last_seen: None,
                online: true,
            },
        ];

        let resolved = resolve_device_target("laptop", self_id, &devices).unwrap();
        assert_eq!(resolved.device_id, target_id);
    }

    #[test]
    fn resolve_device_target_rejects_self() {
        let self_id = DeviceId::new();
        let devices = vec![DeviceInfo {
            device_id: self_id,
            name: "local".to_owned(),
            iroh_public_key: "key".to_owned(),
            enabled_features: vec![],
            last_seen: None,
            online: true,
        }];

        assert!(resolve_device_target("local", self_id, &devices).is_err());
    }
}
