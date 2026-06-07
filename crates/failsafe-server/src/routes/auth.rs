use crate::auth::{hash_password, verify_password};
use crate::entity::{Account, RecoveryCode, account, recovery_code};
use crate::error::{ServerError, ServerResult};
use crate::refresh_token::{self, issue_auth_response};
use crate::state::AppState;
use crate::totp::{
    build_otpauth_uri, decrypt_secret, encrypt_secret, generate_recovery_codes,
    generate_totp_secret, hash_recovery_code, verify_totp,
};
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use chrono::Utc;
use failsafe_core::api::{
    AccountId, AccountResponse, AuthLoginRequest, AuthLogoutRequest, AuthMfaLoginRequest,
    AuthRefreshRequest, AuthRegisterRequest, AuthResponse, ChangePasswordRequest,
    TotpDisableRequest, TotpEnableRequest, TotpEnableResponse, TotpSetupResponse,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set, TransactionTrait,
};
use sea_orm::sea_query::Expr;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/login/mfa", post(login_mfa))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
}

pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/me", get(me))
        .route("/password", post(change_password))
        .route("/2fa/setup", post(totp_setup))
        .route("/2fa/enable", post(totp_enable))
        .route("/2fa/disable", post(totp_disable))
}

async fn me(
    State(state): State<AppState>,
    Extension(account_id): Extension<AccountId>,
) -> ServerResult<Json<AccountResponse>> {
    let account = load_account(account_id, &state).await?;
    Ok(Json(AccountResponse {
        email: account.email,
        totp_enabled: account.totp_enabled,
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

    if request.password.len() < 8 {
        return Err(ServerError::BadRequest(
            "password must be at least 8 characters".to_owned(),
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
        totp_secret: Set(None),
        totp_enabled: Set(false),
        created_at: Set(now),
    }
    .insert(&state.db)
    .await?;

    Ok(Json(issue_auth_response(&state, account_id).await?))
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

    if account.totp_enabled {
        let mfa_token = state.jwt.issue_mfa_pending(AccountId(account.id))?;
        return Ok(Json(AuthResponse::mfa_required(mfa_token)));
    }

    Ok(Json(
        issue_auth_response(&state, AccountId(account.id)).await?,
    ))
}

async fn login_mfa(
    State(state): State<AppState>,
    Json(request): Json<AuthMfaLoginRequest>,
) -> ServerResult<Json<AuthResponse>> {
    if request.mfa_token.trim().is_empty() || request.code.trim().is_empty() {
        return Err(ServerError::BadRequest(
            "mfa_token and code are required".to_owned(),
        ));
    }

    let account_id = state.jwt.validate_mfa_pending(request.mfa_token.trim())?;
    let account = load_account(account_id, &state).await?;

    if !account.totp_enabled {
        return Err(ServerError::Unauthorized);
    }

    let code = request.code.trim();
    let mut verified = false;

    if let Some(encrypted_secret) = &account.totp_secret {
        let secret = decrypt_secret(&state.encryption_key, encrypted_secret)?;
        if verify_totp(&account.email, &secret, code).is_ok() {
            verified = true;
        }
    }

    if !verified {
        verified = consume_recovery_code(&state, account_id, code).await?;
    }

    if !verified {
        return Err(ServerError::Unauthorized);
    }

    Ok(Json(issue_auth_response(&state, account_id).await?))
}

async fn refresh(
    State(state): State<AppState>,
    Json(request): Json<AuthRefreshRequest>,
) -> ServerResult<Json<AuthResponse>> {
    if request.refresh_token.trim().is_empty() {
        return Err(ServerError::BadRequest(
            "refresh_token is required".to_owned(),
        ));
    }

    Ok(Json(
        refresh_token::rotate(&state, request.refresh_token.trim()).await?,
    ))
}

async fn logout(
    State(state): State<AppState>,
    Json(request): Json<AuthLogoutRequest>,
) -> ServerResult<StatusCode> {
    if !request.refresh_token.trim().is_empty() {
        refresh_token::logout(&state, request.refresh_token.trim()).await?;
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn change_password(
    State(state): State<AppState>,
    Extension(account_id): Extension<AccountId>,
    Json(request): Json<ChangePasswordRequest>,
) -> ServerResult<StatusCode> {
    if request.current_password.is_empty() || request.new_password.is_empty() {
        return Err(ServerError::BadRequest(
            "current_password and new_password are required".to_owned(),
        ));
    }

    if request.new_password.len() < 8 {
        return Err(ServerError::BadRequest(
            "new password must be at least 8 characters".to_owned(),
        ));
    }

    let account = load_account(account_id, &state).await?;
    verify_password(&request.current_password, &account.password_hash)?;

    let password_hash = hash_password(&request.new_password)?;
    let txn = state.db.begin().await?;
    let mut active: account::ActiveModel = account.into();
    active.password_hash = Set(password_hash);
    active.update(&txn).await?;
    refresh_token::revoke_all_for_account(&txn, account_id).await?;
    txn.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn totp_setup(
    State(state): State<AppState>,
    Extension(account_id): Extension<AccountId>,
) -> ServerResult<Json<TotpSetupResponse>> {
    let account = load_account(account_id, &state).await?;

    if account.totp_enabled {
        return Err(ServerError::Conflict("2FA is already enabled".to_owned()));
    }

    let secret = generate_totp_secret();
    let encrypted = encrypt_secret(&state.encryption_key, &secret)?;
    let otpauth_uri = build_otpauth_uri(&account.email, &secret)?;

    let mut active: account::ActiveModel = account.into();
    active.totp_secret = Set(Some(encrypted));
    active.totp_enabled = Set(false);
    active.update(&state.db).await?;

    Ok(Json(TotpSetupResponse {
        otpauth_uri,
        secret,
    }))
}

async fn totp_enable(
    State(state): State<AppState>,
    Extension(account_id): Extension<AccountId>,
    Json(request): Json<TotpEnableRequest>,
) -> ServerResult<Json<TotpEnableResponse>> {
    if request.code.trim().is_empty() {
        return Err(ServerError::BadRequest("code is required".to_owned()));
    }

    let account = load_account(account_id, &state).await?;

    if account.totp_enabled {
        return Err(ServerError::Conflict("2FA is already enabled".to_owned()));
    }

    let encrypted_secret = account
        .totp_secret
        .as_deref()
        .ok_or(ServerError::BadRequest(
            "call /auth/2fa/setup before enabling 2FA".to_owned(),
        ))?;
    let secret = decrypt_secret(&state.encryption_key, encrypted_secret)?;
    verify_totp(&account.email, &secret, &request.code)?;

    let recovery_codes = generate_recovery_codes();
    let now = Utc::now();
    let txn = state.db.begin().await?;

    RecoveryCode::delete_many()
        .filter(recovery_code::Column::AccountId.eq(account_id.0))
        .exec(&txn)
        .await?;

    for code in &recovery_codes {
        recovery_code::ActiveModel {
            id: Set(Uuid::new_v4()),
            account_id: Set(account_id.0),
            code_hash: Set(hash_recovery_code(code)),
            used_at: Set(None),
            created_at: Set(now),
        }
        .insert(&txn)
        .await?;
    }

    let mut active: account::ActiveModel = account.into();
    active.totp_enabled = Set(true);
    active.update(&txn).await?;

    txn.commit().await?;

    Ok(Json(TotpEnableResponse { recovery_codes }))
}

async fn totp_disable(
    State(state): State<AppState>,
    Extension(account_id): Extension<AccountId>,
    Json(request): Json<TotpDisableRequest>,
) -> ServerResult<StatusCode> {
    if request.password.is_empty() || request.code.trim().is_empty() {
        return Err(ServerError::BadRequest(
            "password and code are required".to_owned(),
        ));
    }

    let account = load_account(account_id, &state).await?;

    if !account.totp_enabled {
        return Err(ServerError::BadRequest("2FA is not enabled".to_owned()));
    }

    verify_password(&request.password, &account.password_hash)?;

    let code = request.code.trim();
    let mut verified = false;

    if let Some(encrypted_secret) = &account.totp_secret {
        let secret = decrypt_secret(&state.encryption_key, encrypted_secret)?;
        if verify_totp(&account.email, &secret, code).is_ok() {
            verified = true;
        }
    }

    if !verified {
        verified = consume_recovery_code(&state, account_id, code).await?;
    }

    if !verified {
        return Err(ServerError::Unauthorized);
    }

    let txn = state.db.begin().await?;

    RecoveryCode::delete_many()
        .filter(recovery_code::Column::AccountId.eq(account_id.0))
        .exec(&txn)
        .await?;

    let mut active: account::ActiveModel = account.into();
    active.totp_secret = Set(None);
    active.totp_enabled = Set(false);
    active.update(&txn).await?;
    refresh_token::revoke_all_for_account(&txn, account_id).await?;

    txn.commit().await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn load_account(account_id: AccountId, state: &AppState) -> ServerResult<account::Model> {
    Account::find_by_id(account_id.0)
        .one(&state.db)
        .await?
        .ok_or(ServerError::Unauthorized)
}

async fn consume_recovery_code(
    state: &AppState,
    account_id: AccountId,
    code: &str,
) -> ServerResult<bool> {
    let hashed = hash_recovery_code(code);
    let now = Utc::now();
    let result = RecoveryCode::update_many()
        .col_expr(recovery_code::Column::UsedAt, Expr::value(Some(now)))
        .filter(recovery_code::Column::AccountId.eq(account_id.0))
        .filter(recovery_code::Column::CodeHash.eq(hashed))
        .filter(recovery_code::Column::UsedAt.is_null())
        .exec(&state.db)
        .await?;

    Ok(result.rows_affected > 0)
}
