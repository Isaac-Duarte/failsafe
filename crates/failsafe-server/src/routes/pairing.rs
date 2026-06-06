use axum::extract::State;
use axum::routing::post;
use axum::{Extension, Json, Router};
use chrono::{Duration, Utc};
use failsafe_core::api::{AccountId, AuthResponse, PairingCreateResponse, PairingRedeemRequest};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

use crate::entity::{PairingCode, pairing_code};
use crate::error::{ServerError, ServerResult};
use crate::pairing::{generate_code, normalize_code};
use crate::refresh_token::issue_auth_response;
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
            "pairing code must be 6 uppercase alphanumeric characters".to_owned(),
        )
    })?;

    let record = PairingCode::find()
        .filter(pairing_code::Column::Code.eq(code))
        .one(&state.db)
        .await?
        .ok_or_else(|| ServerError::BadRequest("invalid pairing code".to_owned()))?;

    if record.used_at.is_some() {
        return Err(ServerError::BadRequest(
            "pairing code already used".to_owned(),
        ));
    }

    if record.expires_at < Utc::now() {
        return Err(ServerError::BadRequest("pairing code expired".to_owned()));
    }

    let account_id = AccountId(record.account_id);
    let mut active: pairing_code::ActiveModel = record.into();
    active.used_at = Set(Some(Utc::now()));
    active.update(&state.db).await?;

    Ok(Json(issue_auth_response(&state, account_id).await?))
}
