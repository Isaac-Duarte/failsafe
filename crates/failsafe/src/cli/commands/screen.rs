use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::sync::mpsc;

use failsafe::DaemonError;
use failsafe_core::control::{connect_control, ControlStream};
use failsafe_core::screen::ScreenInfo;
use failsafe_screen::{read_nal_from, DecodedFrame, H264Decoder, run_viewer};
use inquire::Select;

use failsafe::control::{
    ControlRequest, ControlResponse, control_socket_path, map_control_connect_error, recv_response,
    send_request,
};

use crate::cli::context::{config_path_or_default, load_config, server_client_from_config};
use crate::cli::device_select::select_device_interactive;
use crate::cli::util::resolve_device_target;

pub async fn screen(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
    device: Option<String>,
) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;
    let config = load_config(&path, server_url.clone(), false)?;
    let client = server_client_from_config(Some(path.clone()), server_url).await?;
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

    let screens = {
        let mut stream = connect_control(&control_socket_path()?)
            .await
            .map_err(map_control_connect_error)?;

        send_request(
            &mut stream,
            &ControlRequest::ListScreens {
                target: target.device_id,
            },
        )
        .await?;

        match recv_response(&mut stream).await? {
            ControlResponse::ScreenList { screens } => screens,
            ControlResponse::Error { message } => return Err(DaemonError::Config(message)),
            _ => {
                return Err(DaemonError::Config(
                    "unexpected response while listing screens".to_owned(),
                ));
            }
        }
    };

    let selected = select_screen_interactive(&screens)?;

    let mut stream = connect_control(&control_socket_path()?)
        .await
        .map_err(map_control_connect_error)?;

    send_request(
        &mut stream,
        &ControlRequest::OpenScreenShare {
            target: target.device_id,
            screen_id: selected.id,
        },
    )
    .await?;

    match recv_response(&mut stream).await? {
        ControlResponse::Ready => {}
        ControlResponse::Error { message } => return Err(DaemonError::Config(message)),
        _ => {
            return Err(DaemonError::Config(
                "unexpected response while opening screen share".to_owned(),
            ));
        }
    }

    let (frame_tx, frame_rx) = mpsc::channel();
    let decode_handle = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .expect("build screen decode runtime");
        if let Err(error) = runtime.block_on(decode_screen_stream(stream, frame_tx)) {
            eprintln!("screen decode exited: {error}");
        }
    });

    run_viewer(frame_rx).map_err(|error| DaemonError::Config(error.to_string()))?;

    let _ = decode_handle.join();
    Ok(())
}

async fn decode_screen_stream(
    mut stream: ControlStream,
    frame_tx: mpsc::Sender<DecodedFrame>,
) -> Result<(), DaemonError> {
    let mut decoder = H264Decoder::new().map_err(|error| DaemonError::Config(error.to_string()))?;

    loop {
        let Some(nal) = read_nal_from(&mut stream)
            .await
            .map_err(DaemonError::Io)?
        else {
            break;
        };

        if let Some(frame) = decoder
            .decode_nal(&nal)
            .map_err(|error| DaemonError::Config(error.to_string()))?
            && frame_tx.send(frame).is_err()
        {
            break;
        }
    }

    Ok(())
}

fn select_screen_interactive(screens: &[ScreenInfo]) -> Result<ScreenInfo, DaemonError> {
    if screens.is_empty() {
        return Err(DaemonError::Config(
            "remote device reported no captureable displays".to_owned(),
        ));
    }

    if screens.len() == 1 {
        return Ok(screens[0].clone());
    }

    if !io::stdin().is_terminal() {
        return Err(DaemonError::Config(
            "multiple displays available; run from a terminal to choose one".to_owned(),
        ));
    }

    let options: Vec<(String, usize)> = screens
        .iter()
        .enumerate()
        .map(|(index, screen)| {
            let label = if screen.width == 0 && screen.height == 0 {
                screen.name.clone()
            } else {
                format!("{}  ({}x{})", screen.name, screen.width, screen.height)
            };
            (label, index)
        })
        .collect();
    let labels: Vec<String> = options.iter().map(|(label, _)| label.clone()).collect();

    let selection = Select::new("Select monitor:", labels)
        .with_help_message("↑/↓ to navigate, enter to select, ctrl+c to cancel")
        .prompt()
        .map_err(|error| DaemonError::Config(error.to_string()))?;

    let index = options
        .iter()
        .find(|(label, _)| label == &selection)
        .map(|(_, index)| *index)
        .expect("selected option must exist");

    Ok(screens[index].clone())
}
