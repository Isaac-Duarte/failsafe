use std::io::{self, IsTerminal};
use std::path::PathBuf;

use failsafe::DaemonError;
use failsafe_core::control::connect_control;
use failsafe_core::control::{ControlEvent, SendPhase};
use failsafe_send::{collect_file_preview, format_bytes};
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
    device: Option<String>,
    yes: bool,
) -> Result<(), DaemonError> {
    if paths.is_empty() {
        return Err(DaemonError::Config(
            "at least one file or directory path is required".to_owned(),
        ));
    }

    let path = config_path_or_default(config_path)?;
    let config = load_config(&path, server_url.clone(), false)?;
    let client = server_client_from_config(Some(path), server_url).await?;
    let response = client.list_devices().await?;

    let target = match device {
        Some(name) => resolve_device_target(&name, config.device_id, &response.devices)?,
        None => select_device_interactive(config.device_id, &response.devices)?,
    };

    if !target.online {
        return Err(DaemonError::Config(format!(
            "device {} is offline",
            target.name
        )));
    }

    let previews = collect_file_preview(&paths)
        .map_err(|error| DaemonError::Config(error))?;
    let total_bytes: u64 = previews.iter().map(|preview| preview.size).sum();

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

    let transfer_id = Uuid::new_v4();
    let mut stream = connect_control(&control_socket_path()?)
        .await
        .map_err(map_control_connect_error)?;

    send_request(
        &mut stream,
        &ControlRequest::SendFiles {
            target: target.device_id,
            paths,
            transfer_id,
        },
    )
    .await?;

    match recv_response(&mut stream).await? {
        ControlResponse::Ready => {}
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

    loop {
        let event = read_event(&mut stream).await.map_err(DaemonError::Control)?;
        match event {
            ControlEvent::SendProgress {
                phase,
                bytes_done,
                bytes_total,
                current_file,
            } => {
                let label = match phase {
                    SendPhase::Preparing => current_file
                        .map(|name| format!("Reading {name}"))
                        .unwrap_or_else(|| "Reading files".to_owned()),
                    SendPhase::Storing => "Storing".to_owned(),
                    SendPhase::Sending => "Sending".to_owned(),
                    SendPhase::WaitingForAck => "Waiting for receiver".to_owned(),
                };
                progress.set_length(bytes_total.max(1));
                progress.set_position(bytes_done.min(bytes_total.max(1)));
                progress.set_message(label);
            }
            ControlEvent::SendComplete { .. } => {
                progress.finish_with_message("done");
                println!("Sent to {}", target.name);
                return Ok(());
            }
            ControlEvent::SendFailed { message } => {
                progress.abandon_with_message("failed");
                return Err(DaemonError::Config(message));
            }
        }
    }
}
