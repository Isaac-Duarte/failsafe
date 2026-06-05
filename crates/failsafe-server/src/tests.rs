use axum::body::Body;
use chrono::{Duration, Utc};
use failsafe_core::api::{
    AuthLoginRequest, AuthRegisterRequest, AuthResponse, DeviceInfo, DeviceListResponse,
    DevicePatchRequest, DeviceUpsertRequest, PairingCreateResponse, PairingRedeemRequest,
};
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use http_body_util::BodyExt;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use tower::ServiceExt;

use crate::auth::JwtService;
use crate::connect_and_migrate;
use crate::entity::{Device, device};
use crate::{AppState, build_app};

async fn body_json<T: serde::de::DeserializeOwned>(body: Body) -> T {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn test_app_with_db() -> (axum::Router, sea_orm::DatabaseConnection) {
    let db = connect_and_migrate("sqlite::memory:")
        .await
        .expect("in-memory database should initialize");
    let state = AppState {
        db: db.clone(),
        jwt: JwtService::new("integration-test-secret"),
    };
    (build_app(state), db)
}

async fn test_app() -> axum::Router {
    test_app_with_db().await.0
}

async fn upsert_test_device(app: &axum::Router, token: &str, device_id: DeviceId) {
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/devices/{device_id}"))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::to_string(&DeviceUpsertRequest {
                        device_id,
                        name: "laptop".to_owned(),
                        iroh_public_key: "abc123".to_owned(),
                        enabled_features: vec![FeatureId::Clipboard],
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn register_login_and_manage_devices() {
    let app = test_app().await;

    let register_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthRegisterRequest {
                        email: "user@example.com".to_owned(),
                        password: "hunter2".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(register_response.status(), axum::http::StatusCode::OK);
    let AuthResponse { token } = body_json(register_response.into_body()).await;

    let login_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthLoginRequest {
                        email: "user@example.com".to_owned(),
                        password: "hunter2".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(login_response.status(), axum::http::StatusCode::OK);
    let AuthResponse { token: login_token } = body_json(login_response.into_body()).await;
    assert!(!login_token.is_empty());
    assert!(!token.is_empty());

    let device_id = DeviceId::new();
    let upsert_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/devices/{device_id}"))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::to_string(&DeviceUpsertRequest {
                        device_id,
                        name: "laptop".to_owned(),
                        iroh_public_key: "abc123".to_owned(),
                        enabled_features: vec![FeatureId::Clipboard],
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(upsert_response.status(), axum::http::StatusCode::OK);

    let list_response = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/v1/devices")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(list_response.status(), axum::http::StatusCode::OK);
    let DeviceListResponse { devices } = body_json(list_response.into_body()).await;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].device_id, device_id);
    assert_eq!(devices[0].iroh_public_key, "abc123");
}

#[tokio::test]
async fn pairing_code_can_be_redeemed_once() {
    let app = test_app().await;

    let register_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthRegisterRequest {
                        email: "pair@example.com".to_owned(),
                        password: "hunter2".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let AuthResponse { token } = body_json(register_response.into_body()).await;

    let create_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/pairing")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(create_response.status(), axum::http::StatusCode::OK);
    let PairingCreateResponse { code, expires_at } = body_json(create_response.into_body()).await;
    assert_eq!(code.len(), 6);
    assert!(
        code.chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
    );
    assert!(!expires_at.is_empty());

    let redeem_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/pairing/redeem")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PairingRedeemRequest { code: code.clone() }).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(redeem_response.status(), axum::http::StatusCode::OK);
    let AuthResponse {
        token: redeemed_token,
    } = body_json(redeem_response.into_body()).await;
    assert!(!redeemed_token.is_empty());

    let list_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/v1/devices")
                .header("authorization", format!("Bearer {redeemed_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(list_response.status(), axum::http::StatusCode::OK);

    let redeem_again_response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/pairing/redeem")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PairingRedeemRequest { code }).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        redeem_again_response.status(),
        axum::http::StatusCode::BAD_REQUEST
    );
}

async fn register_and_get_token(app: &axum::Router) -> String {
    let register_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthRegisterRequest {
                        email: format!("user-{}@example.com", uuid::Uuid::new_v4()),
                        password: "hunter2".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(register_response.status(), axum::http::StatusCode::OK);
    let AuthResponse { token } = body_json(register_response.into_body()).await;
    token
}

#[tokio::test]
async fn patch_rename_and_features() {
    let app = test_app().await;
    let token = register_and_get_token(&app).await;
    let device_id = DeviceId::new();

    app.clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/devices/{device_id}"))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::to_string(&DeviceUpsertRequest {
                        device_id,
                        name: "laptop".to_owned(),
                        iroh_public_key: "abc123".to_owned(),
                        enabled_features: vec![FeatureId::Clipboard],
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let patch_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/devices/{device_id}"))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::to_string(&DevicePatchRequest {
                        name: Some("work-laptop".to_owned()),
                        enabled_features: Some(vec![]),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(patch_response.status(), axum::http::StatusCode::OK);
    let DeviceInfo {
        name,
        enabled_features,
        ..
    } = body_json(patch_response.into_body()).await;
    assert_eq!(name, "work-laptop");
    assert!(enabled_features.is_empty());
}

#[tokio::test]
async fn delete_hides_device_and_blocks_upsert() {
    let app = test_app().await;
    let token = register_and_get_token(&app).await;
    let device_id = DeviceId::new();

    app.clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/devices/{device_id}"))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::to_string(&DeviceUpsertRequest {
                        device_id,
                        name: "laptop".to_owned(),
                        iroh_public_key: "abc123".to_owned(),
                        enabled_features: vec![FeatureId::Clipboard],
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let delete_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/devices/{device_id}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(delete_response.status(), axum::http::StatusCode::OK);

    let list_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/v1/devices")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let DeviceListResponse { devices } = body_json(list_response.into_body()).await;
    assert!(devices.is_empty());

    let upsert_response = app
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/devices/{device_id}"))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::to_string(&DeviceUpsertRequest {
                        device_id,
                        name: "laptop".to_owned(),
                        iroh_public_key: "abc123".to_owned(),
                        enabled_features: vec![FeatureId::Clipboard],
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(upsert_response.status(), axum::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn heartbeat_does_not_revert_patched_name() {
    let app = test_app().await;
    let token = register_and_get_token(&app).await;
    let device_id = DeviceId::new();

    app.clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/devices/{device_id}"))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::to_string(&DeviceUpsertRequest {
                        device_id,
                        name: "laptop".to_owned(),
                        iroh_public_key: "abc123".to_owned(),
                        enabled_features: vec![FeatureId::Clipboard],
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    app.clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/devices/{device_id}"))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::to_string(&DevicePatchRequest {
                        name: Some("renamed".to_owned()),
                        enabled_features: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    app.clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/devices/{device_id}"))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::to_string(&DeviceUpsertRequest {
                        device_id,
                        name: "laptop".to_owned(),
                        iroh_public_key: "updated-key".to_owned(),
                        enabled_features: vec![FeatureId::Clipboard],
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let list_response = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/v1/devices")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let DeviceListResponse { devices } = body_json(list_response.into_body()).await;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].name, "renamed");
    assert_eq!(devices[0].iroh_public_key, "updated-key");
}

#[tokio::test]
async fn heartbeat_updates_online_status() {
    let app = test_app().await;
    let token = register_and_get_token(&app).await;
    let device_id = DeviceId::new();

    upsert_test_device(&app, &token, device_id).await;

    let heartbeat_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri(format!("/api/v1/devices/{device_id}/heartbeat"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(heartbeat_response.status(), axum::http::StatusCode::OK);
    let DeviceInfo {
        online, last_seen, ..
    } = body_json(heartbeat_response.into_body()).await;
    assert!(online);
    assert!(last_seen.is_some());
}

#[tokio::test]
async fn stale_device_is_offline() {
    let (app, db) = test_app_with_db().await;
    let token = register_and_get_token(&app).await;
    let device_id = DeviceId::new();

    upsert_test_device(&app, &token, device_id).await;

    let model = Device::find_by_id(device_id.0)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut active: device::ActiveModel = model.into();
    active.last_seen = Set(Some(Utc::now() - Duration::seconds(120)));
    active.update(&db).await.unwrap();

    let list_response = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/v1/devices")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let DeviceListResponse { devices } = body_json(list_response.into_body()).await;
    assert_eq!(devices.len(), 1);
    assert!(!devices[0].online);
}

#[tokio::test]
async fn heartbeat_on_deleted_device_returns_not_found() {
    let app = test_app().await;
    let token = register_and_get_token(&app).await;
    let device_id = DeviceId::new();

    upsert_test_device(&app, &token, device_id).await;

    let delete_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/devices/{device_id}"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(delete_response.status(), axum::http::StatusCode::OK);

    let heartbeat_response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri(format!("/api/v1/devices/{device_id}/heartbeat"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        heartbeat_response.status(),
        axum::http::StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn serves_embedded_frontend() {
    let app = test_app().await;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.starts_with("text/html"));

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&bytes);
    assert!(html.contains("<!doctype html") || html.contains("<!DOCTYPE html"));
}
