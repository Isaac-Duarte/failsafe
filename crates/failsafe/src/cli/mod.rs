mod args;
mod commands;
mod context;
mod device_select;
mod util;

use clap::Parser;
use failsafe::DaemonError;

use args::{Cli, Command};

pub async fn execute() -> Result<(), DaemonError> {
    let cli = Cli::parse();
    let server_url = cli
        .server_url
        .or_else(|| std::env::var("FAILSAFE_SERVER_URL").ok());

    match cli.command {
        Command::Run { config } => commands::run(config, server_url).await,
        Command::Register {
            config,
            email,
            password,
        } => commands::authenticate(config, server_url, email, password, None, true).await,
        Command::Login {
            config,
            email,
            password,
            totp,
        } => commands::authenticate(config, server_url, email, password, totp, false).await,
        Command::Pair { config, code, name } => {
            commands::pair(config, server_url, code, name).await
        }
        Command::Status { config } => commands::status(config, server_url),
        Command::Devices { command } => commands::devices(command, server_url).await,
        Command::Screen { config, device } => {
            commands::screen(config, server_url, device).await
        }
        Command::Shell { config, device } => commands::shell(config, server_url, device).await,
        Command::Send {
            config,
            paths,
            resume,
            device,
            yes,
            cancel_all,
        } => commands::send(config, server_url, paths, resume, device, yes, cancel_all).await,
        Command::Port {
            config,
            port,
            protocol,
            remote_port,
            device,
        } => commands::port(config, server_url, port, protocol, remote_port, device).await,
    }
}
