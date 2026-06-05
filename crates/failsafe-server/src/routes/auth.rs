use crate::auth::{hash_password, verify_password};
use crate::entity::{Account, account};
use crate::error::{ServerError, ServerResult};
use crate::state::AppState;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use chrono::Utc;
use failsafe_core::api::{
    AccountId, AccountResponse, AuthLoginRequest, AuthRegisterRequest, AuthResponse,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
}

pub fn protected_router() -> Router<AppState> {
    Router::new().route("/me", get(me))
}

async fn me(
    State(state): State<AppState>,
    Extension(account_id): Extension<AccountId>,
) -> ServerResult<Json<AccountResponse>> {
    let account = Account::find_by_id(account_id.0)
        .one(&state.db)
        .await?
        .ok_or(ServerError::Unauthorized)?;

    Ok(Json(AccountResponse {
        email: account.email,
    }))
}

async fn register(
    State(state): State<AppState>,
    Json(request): Json<AuthRegisterRequest>,
) -> ServerResult<Json<AuthResponse>> {
    if request.email.trim().is_empty() || request.password.is_empty() {
        return Err(ServerError::BadRequest(
            "email and password are required".to_owned(),
        ));
    }

    let existing = Account::find()
        .filter(account::Column::Email.eq(request.email.trim().to_lowercase()))
        .one(&state.db)
        .await?;

    if existing.is_some() {
        return Err(ServerError::Conflict("email already registered".to_owned()));
    }

    let account_id = AccountId::new();
    let now = Utc::now();
    let password_hash = hash_password(&request.password)?;

    account::ActiveModel {
        id: Set(account_id.0),
        email: Set(request.email.trim().to_lowercase()),
        password_hash: Set(password_hash),
        created_at: Set(now),
    }
    .insert(&state.db)
    .await?;

    let token = state.jwt.issue(account_id)?;
    Ok(Json(AuthResponse { token }))
}

async fn login(
    State(state): State<AppState>,
    Json(request): Json<AuthLoginRequest>,
) -> ServerResult<Json<AuthResponse>> {
    let account = Account::find()
        .filter(account::Column::Email.eq(request.email.trim().to_lowercase()))
        .one(&state.db)
        .await?
        .ok_or(ServerError::Unauthorized)?;

    verify_password(&request.password, &account.password_hash)?;

    let token = state.jwt.issue(AccountId(account.id))?;
    Ok(Json(AuthResponse { token }))
}
