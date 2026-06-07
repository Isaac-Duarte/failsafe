use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::time::Duration;

use failsafe::DaemonError;
use failsafe_core::api::DeviceInfo;
use failsafe_core::control::connect_control;
use failsafe_core::control::{ControlEvent, ControlStream, SendPhase, send_phase_label};
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureSpec;
use failsafe_send::{
    SendControlBody, SendFeatureSpec, SendFilesRequest, collect_file_preview, format_bytes,
    list_incomplete_sends, load_send_state, prepare_send_paths,
};
use indicatif::{ProgressBar, ProgressStyle};
use inquire::Confirm;
use uuid::Uuid;

use failsafe::control::{
    ControlRequest, ControlResponse, control_socket_path, map_control_connect_error, read_event,
    recv_response, send_request,
};

use crate::cli::context::{config_path_or_default, load_config, server_client_from_config};
use crate::cli::device_select::select_device_interactive;
use crate::cli::util::resolve_device_target;

pub async fn send(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
    paths: Vec<PathBuf>,
    resume: Option<Uuid>,
    device: Option<String>,
    yes: bool,
    cancel_all: bool,
) -> Result<(), DaemonError> {
    if cancel_all {
        return cancel_all_transfers(yes).await;
    }

    if resume.is_none() && paths.is_empty() {
        return list_incomplete().await;
    }

    let (transfer_id, resume_send, paths, target_device) = if let Some(transfer_id) = resume {
        let state = load_send_state(transfer_id)
            .await
            .map_err(DaemonError::Config)?;
        (transfer_id, true, state.paths, Some(state.target))
    } else {
        let paths = prepare_send_paths(&paths).map_err(DaemonError::Config)?;
        (Uuid::new_v4(), false, paths, None)
    };

    let path = config_path_or_default(config_path)?;
    let config = load_config(&path, server_url.clone(), false)?;
    let client = server_client_from_config(Some(path), server_url).await?;
    let response = client.list_devices().await?;

    let target = resolve_send_target(
        device,
        target_device,
        config.device_id,
        &response.devices,
    )?;

    if !target.online {
        return Err(DaemonError::Config(format!(
            "device {} is offline",
            target.name
        )));
    }

    let previews = collect_file_preview(&paths).map_err(DaemonError::Config)?;
    let total_bytes: u64 = previews.iter().map(|preview| preview.size).sum();

    let preview_sizes: Vec<_> = previews
        .iter()
        .map(|preview| (preview.name.clone(), preview.size))
        .collect();
    if !confirm_send(
        resume_send,
        transfer_id,
        &preview_sizes,
        total_bytes,
        &target.name,
        yes,
    )? {
        return Ok(());
    }

    let mut stream = connect_control(&control_socket_path()?)
        .await
        .map_err(map_control_connect_error)?;

    send_request(
        &mut stream,
        &ControlRequest::new(
            SendFeatureSpec::feature_id(),
            SendControlBody::SendFiles(SendFilesRequest {
                target: target.device_id,
                paths,
                transfer_id,
                resume: resume_send,
            }),
        )
        .map_err(DaemonError::Control)?,
    )
    .await?;

    expect_ready_response(&mut stream).await?;

    run_send_progress_loop(
        &mut stream,
        transfer_id,
        total_bytes,
        &target.name,
    )
    .await
}

fn resolve_send_target(
    device: Option<String>,
    target_device: Option<DeviceId>,
    local_device_id: DeviceId,
    devices: &[DeviceInfo],
) -> Result<DeviceInfo, DaemonError> {
    match (device, target_device) {
        (Some(name), _) => resolve_device_target(&name, local_device_id, devices),
        (None, Some(device_id)) => {
            let device = devices
                .iter()
                .find(|entry| entry.device_id == device_id)
                .ok_or_else(|| {
                    DaemonError::Config(format!(
                        "resume target device {device_id} is not in your device list"
                    ))
                })?;
            resolve_device_target(&device.name, local_device_id, devices)
        }
        (None, None) => select_device_interactive(local_device_id, devices),
    }
}

fn require_tty_or_yes(yes: bool) -> Result<(), DaemonError> {
    if !yes && !io::stdin().is_terminal() {
        return Err(DaemonError::Config(
            "confirmation required when stdin is not a terminal; pass --yes".to_owned(),
        ));
    }
    Ok(())
}

fn confirm_send(
    resume_send: bool,
    transfer_id: Uuid,
    previews: &[(String, u64)],
    total_bytes: u64,
    target_name: &str,
    yes: bool,
) -> Result<bool, DaemonError> {
    if resume_send {
        println!("Resuming send {transfer_id}");
    }

    println!("Files to send:");
    for (name, size) in previews {
        println!("  {name} ({})", format_bytes(*size));
    }
    println!(
        "Total: {} ({} files)",
        format_bytes(total_bytes),
        previews.len()
    );

    if yes {
        return Ok(true);
    }

    require_tty_or_yes(false)?;

    let confirmed = Confirm::new(&format!(
        "Send {} file(s) ({}) to {}?",
        previews.len(),
        format_bytes(total_bytes),
        target_name
    ))
    .with_default(false)
    .prompt()
    .map_err(|error| DaemonError::Config(error.to_string()))?;

    if !confirmed {
        println!("cancelled");
        return Ok(false);
    }

    Ok(true)
}

async fn expect_ready_response(stream: &mut ControlStream) -> Result<(), DaemonError> {
    match recv_response(stream).await? {
        ControlResponse::Ready => Ok(()),
        ControlResponse::CancelTransfers { .. } => Err(DaemonError::Config(
            "unexpected cancel transfers response".to_owned(),
        )),
        ControlResponse::Error { message } => Err(DaemonError::Config(message)),
    }
}

async fn expect_cancel_transfers_response(stream: &mut ControlStream) -> Result<(), DaemonError> {
    match recv_response(stream).await? {
        ControlResponse::CancelTransfers { sends, receives } => {
            if sends == 0 && receives == 0 {
                println!("No incomplete transfers.");
            } else {
                println!(
                    "Cancelled {sends} incomplete send(s) and {receives} incomplete receive(s)."
                );
            }
            Ok(())
        }
        ControlResponse::Ready => Err(DaemonError::Config(
            "unexpected ready response for cancel transfers".to_owned(),
        )),
        ControlResponse::Error { message } => Err(DaemonError::Config(message)),
    }
}

async fn run_send_progress_loop(
    stream: &mut ControlStream,
    transfer_id: Uuid,
    total_bytes: u64,
    target_name: &str,
) -> Result<(), DaemonError> {
    let progress = ProgressBar::new(total_bytes.max(1));
    progress.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} {msg}",
        )
        .unwrap()
        .progress_chars("##-"),
    );

    let mut last_sequence = 0u64;
    let mut transfer_total_bytes = total_bytes;

    loop {
        let event = read_event(stream)
            .await
            .map_err(DaemonError::Control)?;
        match event {
            ControlEvent::SendProgress {
                sequence,
                phase,
                bytes_done,
                bytes_total,
                current_file,
            } => {
                if sequence <= last_sequence {
                    continue;
                }
                last_sequence = sequence;

                let label = send_phase_label(phase, current_file.as_deref());
                match phase {
                    SendPhase::WaitingForAck => {
                        transfer_total_bytes = bytes_total.max(transfer_total_bytes);
                        progress.set_length(transfer_total_bytes.max(1));
                        progress.set_position(bytes_done.min(transfer_total_bytes));
                        if bytes_total == 0 || bytes_done >= bytes_total {
                            progress.enable_steady_tick(Duration::from_millis(100));
                        } else {
                            progress.disable_steady_tick();
                        }
                    }
                    _ => {
                        progress.disable_steady_tick();
                        if bytes_total > 0 {
                            progress.set_length(bytes_total);
                            progress.set_position(bytes_done.min(bytes_total));
                        }
                    }
                }
                progress.set_message(label);
            }
            ControlEvent::SendComplete { .. } => {
                progress.disable_steady_tick();
                if transfer_total_bytes > 0 {
                    progress.set_position(transfer_total_bytes);
                }
                progress.finish_with_message("done");
                println!("Sent to {target_name}");
                return Ok(());
            }
            ControlEvent::SendFailed { message } => {
                progress.disable_steady_tick();
                if message == "transfer cancelled" {
                    progress.abandon_with_message("cancelled");
                    return Err(DaemonError::Config(message));
                }
                progress.abandon_with_message("failed");
                return Err(DaemonError::Config(format!(
                    "{message}\nResume with: failsafe send --resume {transfer_id}"
                )));
            }
        }
    }
}

async fn cancel_all_transfers(yes: bool) -> Result<(), DaemonError> {
    if !yes {
        require_tty_or_yes(false)?;

        let confirmed = Confirm::new("Cancel all incomplete sends and receives?")
            .with_default(false)
            .prompt()
            .map_err(|error| DaemonError::Config(error.to_string()))?;

        if !confirmed {
            println!("cancelled");
            return Ok(());
        }
    }

    let mut stream = connect_control(&control_socket_path()?)
        .await
        .map_err(map_control_connect_error)?;

    send_request(
        &mut stream,
        &ControlRequest::new(SendFeatureSpec::feature_id(), SendControlBody::CancelAll)
            .map_err(DaemonError::Control)?,
    )
    .await?;

    expect_cancel_transfers_response(&mut stream).await
}

async fn list_incomplete() -> Result<(), DaemonError> {
    let transfers = list_incomplete_sends().await.map_err(DaemonError::Config)?;
    if transfers.is_empty() {
        println!("No incomplete sends.");
        return Ok(());
    }

    println!("Incomplete sends:");
    for transfer in transfers {
        println!(
            "  {} -> {} ({} files, {}, stage: {:?})",
            transfer.transfer_id,
            transfer.target,
            transfer.entries.len(),
            format_bytes(transfer.bytes_total),
            transfer.stage
        );
        println!(
            "    resume: failsafe send --resume {}",
            transfer.transfer_id
        );
    }
    Ok(())
}
