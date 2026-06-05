use std::net::SocketAddr;

use clap::Parser;
use failsafe_sevrer::{connect_and_migrate, default_database_url, AppState};
use failsafe_sevrer::auth::JwtService;
use tracing::info;

#[derive(Parser)]
#[command(name = "failsafe-sevrer", about = "Failsafe registration server")]
struct Cli {
    /// Address to listen on.
    #[arg(long, default_value = "127.0.0.1:8080")]
    listen: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
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
    };
    let app = failsafe_sevrer::build_app(state);

    info!(%cli.listen, database = %database_url, "failsafe registration server starting");
    let listener = tokio::net::TcpListener::bind(cli.listen).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
