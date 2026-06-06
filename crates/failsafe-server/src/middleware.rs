use crate::entity::Account;
use crate::error::ServerError;
use crate::state::AppState;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sea_orm::EntityTrait;

pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, ServerError> {
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(ServerError::Unauthorized)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(ServerError::Unauthorized)?;

    let account_id = state.jwt.validate(token)?;

    let account_exists = Account::find_by_id(account_id.0)
        .one(&state.db)
        .await?
        .is_some();
    if !account_exists {
        return Err(ServerError::Unauthorized);
    }

    request.extensions_mut().insert(account_id);
    Ok(next.run(request).await)
}

pub async fn unauthorized_response() -> impl IntoResponse {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({ "error": "unauthorized" })),
    )
}
