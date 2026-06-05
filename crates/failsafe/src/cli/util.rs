use std::str::FromStr;

use failsafe::DaemonError;
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;

pub fn parse_device_id(id: &str) -> Result<DeviceId, DaemonError> {
    DeviceId::from_str(id.trim()).map_err(|error| {
        DaemonError::Config(format!("invalid device id `{id}`: {error}"))
    })
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
}
