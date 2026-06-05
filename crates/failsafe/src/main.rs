mod cli;

use failsafe::DaemonError;

#[tokio::main]
async fn main() -> Result<(), DaemonError> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    cli::execute().await
}
