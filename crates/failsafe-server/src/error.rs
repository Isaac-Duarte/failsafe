use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("unauthorized")]
    Unauthorized,

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("not found")]
    NotFound,

    #[error("forbidden")]
    Forbidden,

    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),

    #[error(transparent)]
    Auth(#[from] AuthError),

    #[error("{0}")]
    Internal(String),
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,

    #[error("invalid token")]
    InvalidToken,

    #[error("password hashing failed")]
    HashingFailed,

    #[error("jwt error: {0}")]
    Jwt(String),
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ServerError::Unauthorized | ServerError::Auth(AuthError::InvalidCredentials) => {
                (StatusCode::UNAUTHORIZED, self.to_string())
            }
            ServerError::Auth(AuthError::InvalidToken) => {
                (StatusCode::UNAUTHORIZED, self.to_string())
            }
            ServerError::Conflict(message) => (StatusCode::CONFLICT, message.clone()),
            ServerError::BadRequest(message) => (StatusCode::BAD_REQUEST, message.clone()),
            ServerError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            ServerError::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            ServerError::Database(_) | ServerError::Auth(_) | ServerError::Internal(_) => {
                tracing::error!("{self}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_owned(),
                )
            }
        };

        (status, Json(ErrorBody { error: message })).into_response()
    }
}

pub type ServerResult<T> = Result<T, ServerError>;
