use axum::extract::State;
use axum::routing::post;
use axum::{Extension, Json, Router};
use chrono::{Duration, Utc};
use failsafe_core::api::{AccountId, AuthResponse, PairingCreateResponse, PairingRedeemRequest};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set, TransactionTrait,
};
use sea_orm::sea_query::Expr;
use uuid::Uuid;

use crate::entity::{PairingCode, pairing_code};
use crate::error::{ServerError, ServerResult};
use crate::pairing::{generate_code, normalize_code};
use crate::refresh_token::issue_auth_response;
use crate::routes::devices::{RegisterDeviceMode, register_device};
use crate::state::AppState;

const PAIRING_TTL_MINUTES: i64 = 10;

pub fn public_router() -> Router<AppState> {
    Router::new().route("/redeem", post(redeem_pairing_code))
}

pub fn protected_router() -> Router<AppState> {
    Router::new().route("/", post(create_pairing_code))
}

async fn create_pairing_code(
    State(state): State<AppState>,
    Extension(account_id): Extension<AccountId>,
) -> ServerResult<Json<PairingCreateResponse>> {
    let now = Utc::now();
    let expires_at = now + Duration::minutes(PAIRING_TTL_MINUTES);

    for _ in 0..10 {
        let code = generate_code();
        let result = pairing_code::ActiveModel {
            id: Set(Uuid::new_v4()),
            account_id: Set(account_id.0),
            code: Set(code.clone()),
            expires_at: Set(expires_at),
            used_at: Set(None),
            created_at: Set(now),
        }
        .insert(&state.db)
        .await;

        match result {
            Ok(_) => {
                return Ok(Json(PairingCreateResponse {
                    code,
                    expires_at: expires_at.to_rfc3339(),
                }));
            }
            Err(_) => continue,
        }
    }

    Err(ServerError::Internal(
        "failed to generate unique pairing code".to_owned(),
    ))
}

async fn redeem_pairing_code(
    State(state): State<AppState>,
    Json(request): Json<PairingRedeemRequest>,
) -> ServerResult<Json<AuthResponse>> {
    let code = normalize_code(&request.code).ok_or_else(|| {
        ServerError::BadRequest(
            "pairing code must be 8 uppercase alphanumeric characters".to_owned(),
        )
    })?;

    let txn = state.db.begin().await?;

    let record = PairingCode::find()
        .filter(pairing_code::Column::Code.eq(code))
        .one(&txn)
        .await?
        .ok_or_else(|| ServerError::BadRequest("invalid pairing code".to_owned()))?;

    if record.expires_at < Utc::now() {
        return Err(ServerError::BadRequest("pairing code expired".to_owned()));
    }

    if record.used_at.is_some() {
        return Err(ServerError::BadRequest(
            "pairing code already used".to_owned(),
        ));
    }

    let account_id = AccountId(record.account_id);

    if let Some(device) = request.device {
        register_device(&txn, account_id, device, RegisterDeviceMode::Pairing).await?;
    }

    let now = Utc::now();
    let result = pairing_code::Entity::update_many()
        .col_expr(pairing_code::Column::UsedAt, Expr::value(Some(now)))
        .filter(pairing_code::Column::Id.eq(record.id))
        .filter(pairing_code::Column::UsedAt.is_null())
        .exec(&txn)
        .await?;

    if result.rows_affected == 0 {
        return Err(ServerError::BadRequest(
            "pairing code already used".to_owned(),
        ));
    }

    txn.commit().await?;

    Ok(Json(issue_auth_response(&state, account_id).await?))
}
