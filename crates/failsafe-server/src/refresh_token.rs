use chrono::{Duration, Utc};
use failsafe_core::api::{AccountId, AuthResponse};
use rand::RngCore;
use rand::rngs::OsRng;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction,
    EntityTrait, QueryFilter, Set, TransactionTrait,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::JwtService;
use crate::entity::{Account, RefreshToken, refresh_token};
use crate::error::ServerError;
use crate::state::AppState;

pub const REFRESH_TOKEN_TTL: Duration = Duration::days(30);

pub fn hash_token(raw: &str) -> String {
    let hash = Sha256::digest(raw.as_bytes());
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn generate_raw_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub async fn issue(db: &DatabaseConnection, account_id: AccountId) -> Result<String, ServerError> {
    let raw = generate_raw_token();
    let now = Utc::now();
    refresh_token::ActiveModel {
        id: Set(Uuid::new_v4()),
        account_id: Set(account_id.0),
        token_hash: Set(hash_token(&raw)),
        expires_at: Set(now + REFRESH_TOKEN_TTL),
        revoked_at: Set(None),
        created_at: Set(now),
    }
    .insert(db)
    .await?;
    Ok(raw)
}

async fn find_valid_in_conn<C>(conn: &C, raw: &str) -> Result<refresh_token::Model, ServerError>
where
    C: ConnectionTrait,
{
    let record = RefreshToken::find()
        .filter(refresh_token::Column::TokenHash.eq(hash_token(raw)))
        .one(conn)
        .await?
        .ok_or(ServerError::Unauthorized)?;

    if record.revoked_at.is_some() || record.expires_at < Utc::now() {
        return Err(ServerError::Unauthorized);
    }

    let account_exists = Account::find_by_id(record.account_id)
        .one(conn)
        .await?
        .is_some();
    if !account_exists {
        return Err(ServerError::Unauthorized);
    }

    Ok(record)
}

pub async fn revoke_record<C>(conn: &C, record: refresh_token::Model) -> Result<(), ServerError>
where
    C: ConnectionTrait,
{
    let mut active: refresh_token::ActiveModel = record.into();
    active.revoked_at = Set(Some(Utc::now()));
    active.update(conn).await?;
    Ok(())
}

pub async fn issue_auth_response(
    state: &AppState,
    account_id: AccountId,
) -> Result<AuthResponse, ServerError> {
    let token = state.jwt.issue(account_id)?;
    let refresh_token = issue(&state.db, account_id).await?;
    Ok(AuthResponse {
        token,
        refresh_token,
    })
}

pub async fn rotate(state: &AppState, raw: &str) -> Result<AuthResponse, ServerError> {
    let txn = state.db.begin().await?;
    let response = rotate_in_transaction(&txn, &state.jwt, raw).await?;
    txn.commit().await?;
    Ok(response)
}

async fn rotate_in_transaction(
    txn: &DatabaseTransaction,
    jwt: &JwtService,
    raw: &str,
) -> Result<AuthResponse, ServerError> {
    let record = find_valid_in_conn(txn, raw).await?;

    revoke_record(txn, record.clone()).await?;

    let account_id = AccountId(record.account_id);
    let new_raw = generate_raw_token();
    let now = Utc::now();
    refresh_token::ActiveModel {
        id: Set(Uuid::new_v4()),
        account_id: Set(account_id.0),
        token_hash: Set(hash_token(&new_raw)),
        expires_at: Set(now + REFRESH_TOKEN_TTL),
        revoked_at: Set(None),
        created_at: Set(now),
    }
    .insert(txn)
    .await?;

    let token = jwt.issue(account_id)?;
    Ok(AuthResponse {
        token,
        refresh_token: new_raw,
    })
}

pub async fn logout(state: &AppState, raw: &str) -> Result<(), ServerError> {
    let record = RefreshToken::find()
        .filter(refresh_token::Column::TokenHash.eq(hash_token(raw)))
        .one(&state.db)
        .await?;

    if let Some(record) = record
        && record.revoked_at.is_none()
    {
        revoke_record(&state.db, record).await?;
    }

    Ok(())
}
