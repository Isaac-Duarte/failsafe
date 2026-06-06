use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::time::Duration;

use failsafe::DaemonError;
use failsafe_core::control::connect_control;
use failsafe_core::control::{send_phase_label, ControlEvent, SendPhase};
use failsafe_send::{
    collect_file_preview, format_bytes, list_incomplete_sends, load_send_state,
};
use indicatif::{ProgressBar, ProgressStyle};
use inquire::Confirm;
use uuid::Uuid;

use failsafe::control::{
    control_socket_path, map_control_connect_error, read_event, recv_response, send_request,
    ControlRequest, ControlResponse,
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
        (Uuid::new_v4(), false, paths, None)
    };

    let path = config_path_or_default(config_path)?;
    let config = load_config(&path, server_url.clone(), false)?;
    let client = server_client_from_config(Some(path), server_url).await?;
    let response = client.list_devices().await?;

    let target = match (device, target_device) {
        (Some(name), _) => resolve_device_target(&name, config.device_id, &response.devices)?,
        (None, Some(device_id)) => {
            let device = response
                .devices
                .iter()
                .find(|entry| entry.device_id == device_id)
                .ok_or_else(|| {
                    DaemonError::Config(format!(
                        "resume target device {device_id} is not in your device list"
                    ))
                })?;
            resolve_device_target(&device.name, config.device_id, &response.devices)?
        }
        (None, None) => select_device_interactive(config.device_id, &response.devices)?,
    };

    if !target.online {
        return Err(DaemonError::Config(format!(
            "device {} is offline",
            target.name
        )));
    }

    let previews = collect_file_preview(&paths).map_err(DaemonError::Config)?;
    let total_bytes: u64 = previews.iter().map(|preview| preview.size).sum();

    if resume_send {
        println!("Resuming send {transfer_id}");
    }

    println!("Files to send:");
    for preview in &previews {
        println!("  {} ({})", preview.name, format_bytes(preview.size));
    }
    println!("Total: {} ({} files)", format_bytes(total_bytes), previews.len());

    if !yes {
        if !io::stdin().is_terminal() {
            return Err(DaemonError::Config(
                "confirmation required when stdin is not a terminal; pass --yes".to_owned(),
            ));
        }

        let confirmed = Confirm::new(&format!(
            "Send {} file(s) ({}) to {}?",
            previews.len(),
            format_bytes(total_bytes),
            target.name
        ))
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
        &ControlRequest::SendFiles {
            target: target.device_id,
            paths,
            transfer_id,
            resume: resume_send,
        },
    )
    .await?;

    match recv_response(&mut stream).await? {
        ControlResponse::Ready => {}
        ControlResponse::CancelTransfers { .. } => {
            return Err(DaemonError::Config(
                "unexpected cancel transfers response".to_owned(),
            ));
        }
        ControlResponse::Error { message } => {
            return Err(DaemonError::Config(message));
        }
    }

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
        let event = read_event(&mut stream).await.map_err(DaemonError::Control)?;
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
                        progress.set_position(transfer_total_bytes);
                        progress.enable_steady_tick(Duration::from_millis(100));
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
                println!("Sent to {}", target.name);
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
        if !io::stdin().is_terminal() {
            return Err(DaemonError::Config(
                "confirmation required when stdin is not a terminal; pass --yes".to_owned(),
            ));
        }

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

    send_request(&mut stream, &ControlRequest::CancelTransfers).await?;

    match recv_response(&mut stream).await? {
        ControlResponse::CancelTransfers { sends, receives } => {
            if sends == 0 && receives == 0 {
                println!("No incomplete transfers.");
            } else {
                println!("Cancelled {sends} incomplete send(s) and {receives} incomplete receive(s).");
            }
            Ok(())
        }
        ControlResponse::Ready => Err(DaemonError::Config(
            "unexpected ready response for cancel transfers".to_owned(),
        )),
        ControlResponse::Error { message } => Err(DaemonError::Config(message)),
    }
}

async fn list_incomplete() -> Result<(), DaemonError> {
    let transfers = list_incomplete_sends()
        .await
        .map_err(DaemonError::Config)?;
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
        println!("    resume: failsafe send --resume {}", transfer.transfer_id);
    }
    Ok(())
}
