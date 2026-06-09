#[cfg(test)]
mod tests;

pub mod auth;
pub mod config;
pub mod entity;
pub mod error;
pub mod middleware;
pub mod migration;
pub mod pairing;
pub mod presence;
pub mod rate_limit;
pub mod refresh_token;
pub mod routes;
pub mod state;
pub mod totp;
pub mod virtual_ip;
pub mod web;

use std::path::PathBuf;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use sea_orm::Database;
use tower_http::trace::TraceLayer;

use crate::auth::JwtService;
use crate::migration::Migrator;
pub use crate::state::AppState;
use sea_orm_migration::MigratorTrait;

pub async fn connect_and_migrate(database_url: &str) -> Result<DatabaseConnection, sea_orm::DbErr> {
    let db = Database::connect(database_url).await?;
    Migrator::up(&db, None).await?;
    Ok(db)
}

pub use sea_orm::DatabaseConnection;

pub fn default_database_url() -> Option<String> {
    if let Ok(url) = std::env::var("FAILSAFE_DB_URL") {
        return Some(url);
    }

    dirs::data_local_dir().map(|dir| {
        let path = dir.join("failsafe").join("failsafe.db");
        format!("sqlite://{}?mode=rwc", path.display())
    })
}

pub fn ensure_database_parent(database_url: &str) -> std::io::Result<()> {
    if let Some(path) = database_url.strip_prefix("sqlite://") {
        let path = path.split('?').next().unwrap_or(path);
        if let Some(parent) = PathBuf::from(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

pub fn load_encryption_key() -> Result<String, String> {
    std::env::var("FAILSAFE_ENCRYPTION_KEY").map_err(|_| {
        "FAILSAFE_ENCRYPTION_KEY environment variable is required".to_owned()
    })
}

pub fn build_app(state: AppState) -> Router {
    let auth_public = routes::auth::router().route_layer(from_fn_with_state(
        state.clone(),
        rate_limit::login_rate_limit_middleware,
    ));

    let pairing_public = routes::pairing::public_router().route_layer(from_fn_with_state(
        state.clone(),
        rate_limit::pairing_rate_limit_middleware,
    ));

    let public = Router::new()
        .nest("/api/v1/auth", auth_public)
        .nest("/api/v1/features", routes::features::router())
        .nest("/api/v1/pairing", pairing_public)
        .route("/health", get(|| async { "ok" }));

    let protected = Router::new()
        .nest("/api/v1/auth", routes::auth::protected_router())
        .nest("/api/v1/devices", routes::devices::router())
        .nest("/api/v1/pairing", routes::pairing::protected_router())
        .layer(from_fn_with_state(state.clone(), middleware::require_auth));

    Router::new()
        .merge(public)
        .merge(protected)
        .fallback(get(web::serve))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn app_from_parts(
    database_url: &str,
    jwt_secret: &str,
    encryption_key: &str,
) -> Result<Router, sea_orm::DbErr> {
    ensure_database_parent(database_url).map_err(|error| {
        sea_orm::DbErr::Custom(format!("failed to create database parent: {error}"))
    })?;
    let db = connect_and_migrate(database_url).await?;
    let state = AppState {
        db,
        jwt: JwtService::new(jwt_secret),
        encryption_key: encryption_key.to_owned(),
        login_limiter: rate_limit::RateLimiter::new(20, std::time::Duration::from_secs(60)),
        pairing_limiter: rate_limit::RateLimiter::new(10, std::time::Duration::from_secs(60)),
    };
    Ok(build_app(state))
}
