use std::net::SocketAddr;

use clap::Parser;
use failsafe_server::auth::JwtService;
use failsafe_server::config::ListenConfig;
use failsafe_server::{AppState, connect_and_migrate, default_database_url};
use tracing::info;

#[derive(Parser)]
#[command(name = "failsafe-server", about = "Failsafe registration server")]
struct Cli {
    /// Full socket address to listen on (overrides --host/--port).
    #[arg(long)]
    listen: Option<SocketAddr>,
    /// IP address or hostname to bind.
    #[arg(long, env = "FAILSAFE_LISTEN_HOST")]
    host: Option<String>,
    /// TCP port to bind.
    #[arg(long, env = "FAILSAFE_LISTEN_PORT")]
    port: Option<u16>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let listen = ListenConfig {
        listen: cli.listen,
        host: cli.host,
        port: cli.port,
    }
    .resolve()
    .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;

    let jwt_secret = std::env::var("FAILSAFE_JWT_SECRET")
        .map_err(|_| "FAILSAFE_JWT_SECRET environment variable is required")?;
    let database_url = default_database_url()
        .ok_or("could not determine default database path for this platform")?;

    if let Some(path) = database_url.strip_prefix("sqlite://") {
        let path = path.split('?').next().unwrap_or(path);
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let db = connect_and_migrate(&database_url).await?;
    let state = AppState {
        db,
        jwt: JwtService::new(&jwt_secret),
        encryption_key: jwt_secret.clone(),
    };
    let app = failsafe_server::build_app(state);

    info!(%listen, database = %database_url, "failsafe registration server starting");
    let listener = tokio::net::TcpListener::bind(listen).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
