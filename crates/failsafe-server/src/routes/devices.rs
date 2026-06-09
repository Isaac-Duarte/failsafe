use axum::extract::{Path, State};
use axum::routing::{delete, get, patch, post, put};
use axum::{Extension, Json, Router};
use chrono::Utc;
use failsafe_core::api::{
    AccountId, DeviceInfo, DeviceListResponse, DevicePatchRequest, DeviceUpsertRequest,
};
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_feature_registry::is_known_feature;
use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set};
use uuid::Uuid;

use crate::entity::{Device, device};
use crate::error::{ServerError, ServerResult};
use crate::presence;
use crate::state::AppState;
use crate::virtual_ip::ensure_virtual_ip;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_devices))
        .route("/{device_id}", put(upsert_device))
        .route("/{device_id}", patch(patch_device))
        .route("/{device_id}", delete(delete_device))
        .route("/{device_id}/heartbeat", post(heartbeat_device))
}

async fn list_devices(
    State(state): State<AppState>,
    Extension(account_id): Extension<AccountId>,
) -> ServerResult<Json<DeviceListResponse>> {
    let devices = Device::find()
        .filter(device::Column::AccountId.eq(account_id.0))
        .filter(device::Column::DeletedAt.is_null())
        .all(&state.db)
        .await?;

    let devices = devices
        .into_iter()
        .map(model_to_info)
        .collect::<ServerResult<Vec<_>>>()?;

    Ok(Json(DeviceListResponse { devices }))
}

async fn upsert_device(
    State(state): State<AppState>,
    Extension(account_id): Extension<AccountId>,
    Path(device_id): Path<Uuid>,
    Json(request): Json<DeviceUpsertRequest>,
) -> ServerResult<Json<DeviceInfo>> {
    if request.device_id.0 != device_id {
        return Err(ServerError::BadRequest(
            "device_id in path and body must match".to_owned(),
        ));
    }

    let model =
        register_device(&state.db, account_id, request, RegisterDeviceMode::Upsert).await?;
    Ok(Json(model_to_info(model)?))
}

pub(crate) enum RegisterDeviceMode {
    Upsert,
    Pairing,
}

pub(crate) async fn register_device<C>(
    conn: &C,
    account_id: AccountId,
    request: DeviceUpsertRequest,
    mode: RegisterDeviceMode,
) -> ServerResult<device::Model>
where
    C: ConnectionTrait,
{
    if request.name.trim().is_empty() || request.iroh_public_key.trim().is_empty() {
        return Err(ServerError::BadRequest(
            "name and iroh_public_key are required".to_owned(),
        ));
    }

    let device_id = request.device_id.0;
    let existing = Device::find_by_id(device_id).one(conn).await?;

    if let Some(existing) = existing {
        if existing.account_id != account_id.0 {
            return Err(ServerError::Forbidden);
        }

        if existing.deleted_at.is_some() {
            return match mode {
                RegisterDeviceMode::Upsert => {
                    Err(ServerError::ForbiddenMessage("device removed".to_owned()))
                }
                RegisterDeviceMode::Pairing => {
                    let now = Utc::now();
                    let virtual_ip = ensure_virtual_ip(
                        conn,
                        account_id.0,
                        device_id,
                        existing.virtual_ip.clone(),
                    )
                    .await?;
                    let mut active: device::ActiveModel = existing.into();
                    active.deleted_at = Set(None);
                    active.name = Set(request.name.trim().to_owned());
                    active.iroh_public_key = Set(request.iroh_public_key.trim().to_owned());
                    active.enabled_features = Set(features_to_json(&request.enabled_features)?);
                    active.virtual_ip = Set(Some(virtual_ip));
                    active.last_seen = Set(Some(now));
                    Ok(active.update(conn).await?)
                }
            };
        }

        // Policy fields (name, enabled_features) are server-authoritative and only
        // change via PATCH. PUT updates transport state for existing devices.
        let virtual_ip = ensure_virtual_ip(
            conn,
            account_id.0,
            device_id,
            existing.virtual_ip.clone(),
        )
        .await?;
        let mut active: device::ActiveModel = existing.into();
        active.iroh_public_key = Set(request.iroh_public_key.trim().to_owned());
        active.virtual_ip = Set(Some(virtual_ip));
        active.last_seen = Set(Some(Utc::now()));
        return Ok(active.update(conn).await?);
    }

    let now = Utc::now();
    let virtual_ip = ensure_virtual_ip(conn, account_id.0, device_id, None).await?;
    Ok(device::ActiveModel {
        device_id: Set(device_id),
        account_id: Set(account_id.0),
        name: Set(request.name.trim().to_owned()),
        iroh_public_key: Set(request.iroh_public_key.trim().to_owned()),
        enabled_features: Set(features_to_json(&request.enabled_features)?),
        virtual_ip: Set(Some(virtual_ip)),
        last_seen: Set(Some(now)),
        created_at: Set(now),
        deleted_at: Set(None),
    }
    .insert(conn)
    .await?)
}

async fn patch_device(
    State(state): State<AppState>,
    Extension(account_id): Extension<AccountId>,
    Path(device_id): Path<Uuid>,
    Json(request): Json<DevicePatchRequest>,
) -> ServerResult<Json<DeviceInfo>> {
    if request.name.is_none() && request.enabled_features.is_none() {
        return Err(ServerError::BadRequest(
            "at least one of name or enabled_features is required".to_owned(),
        ));
    }

    if let Some(name) = &request.name
        && name.trim().is_empty()
    {
        return Err(ServerError::BadRequest("name cannot be empty".to_owned()));
    }

    let existing = load_active_device(device_id, account_id, &state).await?;

    let mut active: device::ActiveModel = existing.into();

    if let Some(name) = request.name {
        active.name = Set(name.trim().to_owned());
    }

    if let Some(features) = request.enabled_features {
        active.enabled_features = Set(features_to_json(&features)?);
    }

    let updated = active.update(&state.db).await?;
    Ok(Json(model_to_info(updated)?))
}

async fn delete_device(
    State(state): State<AppState>,
    Extension(account_id): Extension<AccountId>,
    Path(device_id): Path<Uuid>,
) -> ServerResult<()> {
    let existing = Device::find_by_id(device_id)
        .one(&state.db)
        .await?
        .ok_or(ServerError::NotFound)?;

    if existing.account_id != account_id.0 {
        return Err(ServerError::Forbidden);
    }

    if existing.deleted_at.is_some() {
        return Err(ServerError::NotFound);
    }

    let mut active: device::ActiveModel = existing.into();
    active.deleted_at = Set(Some(Utc::now()));
    active.update(&state.db).await?;

    Ok(())
}

async fn heartbeat_device(
    State(state): State<AppState>,
    Extension(account_id): Extension<AccountId>,
    Path(device_id): Path<Uuid>,
) -> ServerResult<Json<DeviceInfo>> {
    let existing = load_active_device(device_id, account_id, &state).await?;
    let mut active: device::ActiveModel = existing.into();
    active.last_seen = Set(Some(Utc::now()));
    let updated = active.update(&state.db).await?;
    Ok(Json(model_to_info(updated)?))
}

async fn load_active_device(
    device_id: Uuid,
    account_id: AccountId,
    state: &AppState,
) -> ServerResult<device::Model> {
    let existing = Device::find_by_id(device_id)
        .one(&state.db)
        .await?
        .ok_or(ServerError::NotFound)?;

    if existing.account_id != account_id.0 {
        return Err(ServerError::Forbidden);
    }

    if existing.deleted_at.is_some() {
        return Err(ServerError::NotFound);
    }

    Ok(existing)
}

fn model_to_info(model: device::Model) -> ServerResult<DeviceInfo> {
    let enabled_features: Vec<FeatureId> = serde_json::from_value(model.enabled_features)
        .map_err(|error| ServerError::Internal(format!("invalid feature data: {error}")))?;

    Ok(DeviceInfo {
        device_id: DeviceId(model.device_id),
        name: model.name,
        iroh_public_key: model.iroh_public_key,
        enabled_features,
        virtual_ip: model.virtual_ip,
        last_seen: model.last_seen.map(|ts| ts.to_rfc3339()),
        online: presence::is_online(model.last_seen),
    })
}

fn features_to_json(features: &[FeatureId]) -> ServerResult<serde_json::Value> {
    for feature in features {
        if !is_known_feature(feature.as_str()) {
            return Err(ServerError::BadRequest(format!(
                "unknown feature `{}`",
                feature
            )));
        }
    }

    serde_json::to_value(features)
        .map_err(|error| ServerError::Internal(format!("failed to encode features: {error}")))
}
