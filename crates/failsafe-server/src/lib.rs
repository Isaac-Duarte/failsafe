#[cfg(test)]
mod tests;

pub mod auth;
pub mod entity;
pub mod error;
pub mod middleware;
pub mod migration;
pub mod pairing;
pub mod routes;
pub mod state;
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

pub fn build_app(state: AppState) -> Router {
    let public = Router::new()
        .nest("/api/v1/auth", routes::auth::router())
        .nest("/api/v1/pairing", routes::pairing::public_router())
        .route("/health", get(|| async { "ok" }));

    let protected = Router::new()
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
) -> Result<Router, sea_orm::DbErr> {
    ensure_database_parent(database_url).map_err(|error| {
        sea_orm::DbErr::Custom(format!("failed to create database parent: {error}"))
    })?;
    let db = connect_and_migrate(database_url).await?;
    let state = AppState {
        db,
        jwt: JwtService::new(jwt_secret),
    };
    Ok(build_app(state))
}
