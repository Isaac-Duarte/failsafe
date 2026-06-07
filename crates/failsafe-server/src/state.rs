use sea_orm::DatabaseConnection;

use crate::auth::JwtService;
use crate::rate_limit::RateLimiter;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub jwt: JwtService,
    pub encryption_key: String,
    pub login_limiter: RateLimiter,
    pub pairing_limiter: RateLimiter,
}
