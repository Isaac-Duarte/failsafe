use std::io::{self, IsTerminal};

use failsafe::DaemonError;
use failsafe_core::api::DeviceInfo;
use failsafe_core::device::DeviceId;
use inquire::Select;

pub fn select_device_interactive(
    self_id: DeviceId,
    devices: &[DeviceInfo],
) -> Result<DeviceInfo, DaemonError> {
    let candidates: Vec<DeviceInfo> = devices
        .iter()
        .filter(|device| device.device_id != self_id)
        .cloned()
        .collect();

    if candidates.is_empty() {
        return Err(DaemonError::Config(
            "no other devices available to connect to".to_owned(),
        ));
    }

    if !io::stdin().is_terminal() {
        return Err(DaemonError::Config(
            "device name required when stdin is not a terminal".to_owned(),
        ));
    }

    let options: Vec<(String, usize)> = candidates
        .iter()
        .enumerate()
        .map(|(index, device)| {
            let status = if device.online { "online" } else { "offline" };
            (format!("{}  [{status}]", device.name), index)
        })
        .collect();
    let labels: Vec<String> = options.iter().map(|(label, _)| label.clone()).collect();

    let selection = Select::new("Select device:", labels)
        .with_help_message("↑/↓ to navigate, enter to select, ctrl+c to cancel")
        .prompt()
        .map_err(|error| DaemonError::Config(error.to_string()))?;

    let index = options
        .iter()
        .find(|(label, _)| label == &selection)
        .map(|(_, index)| *index)
        .expect("selected option must exist");
    Ok(candidates[index].clone())
}
