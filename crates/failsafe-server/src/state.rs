use sea_orm::DatabaseConnection;

use crate::auth::JwtService;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub jwt: JwtService,
    pub encryption_key: String,
}
