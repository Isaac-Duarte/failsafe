use axum::body::Body;
use chrono::{Duration, Utc};
use failsafe_core::api::{
    AccountId, AccountResponse, AuthLoginRequest, AuthLogoutRequest, AuthMfaLoginRequest,
    AuthRefreshRequest, AuthRegisterRequest, AuthResponse, ChangePasswordRequest, DeviceInfo,
    DeviceListResponse, DevicePatchRequest, DeviceUpsertRequest, PairingCreateResponse,
    PairingRedeemRequest, TotpDisableRequest, TotpEnableRequest, TotpEnableResponse,
    TotpSetupResponse,
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

fn auth_token(auth: &AuthResponse) -> &str {
    auth.token.as_deref().expect("expected auth token")
}

fn refresh_token_value(auth: &AuthResponse) -> &str {
    auth.refresh_token
        .as_deref()
        .expect("expected refresh token")
}

async fn test_app_with_db() -> (axum::Router, sea_orm::DatabaseConnection) {
    let db = connect_and_migrate("sqlite::memory:")
        .await
        .expect("in-memory database should initialize");
    let state = AppState {
        db: db.clone(),
        jwt: JwtService::new("integration-test-secret"),
        encryption_key: "integration-test-secret".to_owned(),
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
async fn auth_me_returns_account_email() {
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
                        email: "me@example.com".to_owned(),
                        password: "hunter22".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(register_response.status(), axum::http::StatusCode::OK);
    let auth: AuthResponse = body_json(register_response.into_body()).await;
    let token = auth_token(&auth);

    let me_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/v1/auth/me")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(me_response.status(), axum::http::StatusCode::OK);
    let AccountResponse {
        email,
        totp_enabled,
    } = body_json(me_response.into_body()).await;
    assert_eq!(email, "me@example.com");
    assert!(!totp_enabled);
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
                        password: "hunter22".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(register_response.status(), axum::http::StatusCode::OK);
    let auth: AuthResponse = body_json(register_response.into_body()).await;
    let token = auth_token(&auth);

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
                        password: "hunter22".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(login_response.status(), axum::http::StatusCode::OK);
    let login_auth: AuthResponse = body_json(login_response.into_body()).await;
    assert!(!auth_token(&login_auth).is_empty());
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
                        password: "hunter22".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let auth: AuthResponse = body_json(register_response.into_body()).await;
    let token = auth_token(&auth);

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
                    serde_json::to_string(&PairingRedeemRequest {
                        code: code.clone(),
                        device: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(redeem_response.status(), axum::http::StatusCode::OK);
    let redeem_auth: AuthResponse = body_json(redeem_response.into_body()).await;
    let redeemed_token = auth_token(&redeem_auth);
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
                    serde_json::to_string(&PairingRedeemRequest { code, device: None }).unwrap(),
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
                        password: "hunter22".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(register_response.status(), axum::http::StatusCode::OK);
    let auth: AuthResponse = body_json(register_response.into_body()).await;
    auth_token(&auth).to_owned()
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

async fn create_pairing_code(app: &axum::Router, token: &str) -> String {
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
    let PairingCreateResponse { code, .. } = body_json(create_response.into_body()).await;
    code
}

#[tokio::test]
async fn deleted_device_can_rejoin_via_pairing_redeem() {
    let (app, db) = test_app_with_db().await;
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

    let code = create_pairing_code(&app, &token).await;

    let redeem_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/pairing/redeem")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PairingRedeemRequest {
                        code,
                        device: Some(DeviceUpsertRequest {
                            device_id,
                            name: "laptop".to_owned(),
                            iroh_public_key: "restored-key".to_owned(),
                            enabled_features: vec![FeatureId::Clipboard],
                        }),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(redeem_response.status(), axum::http::StatusCode::OK);
    let redeem_auth: AuthResponse = body_json(redeem_response.into_body()).await;
    let redeemed_token = auth_token(&redeem_auth);

    let model = Device::find_by_id(device_id.0)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(model.deleted_at.is_none());
    assert_eq!(model.iroh_public_key, "restored-key");

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

    let DeviceListResponse { devices } = body_json(list_response.into_body()).await;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].device_id, device_id);
    assert_eq!(devices[0].iroh_public_key, "restored-key");

    let upsert_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/devices/{device_id}"))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {redeemed_token}"))
                .body(Body::from(
                    serde_json::to_string(&DeviceUpsertRequest {
                        device_id,
                        name: "laptop".to_owned(),
                        iroh_public_key: "restored-key".to_owned(),
                        enabled_features: vec![FeatureId::Clipboard],
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(upsert_response.status(), axum::http::StatusCode::OK);

    let heartbeat_response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri(format!("/api/v1/devices/{device_id}/heartbeat"))
                .header("authorization", format!("Bearer {redeemed_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(heartbeat_response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn redeem_without_device_does_not_restore_deleted_device() {
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

    let code = create_pairing_code(&app, &token).await;

    let redeem_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/pairing/redeem")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PairingRedeemRequest { code, device: None }).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(redeem_response.status(), axum::http::StatusCode::OK);

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
async fn upsert_update_ignores_features_on_existing_device() {
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
                        name: None,
                        enabled_features: Some(vec![]),
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
    assert!(devices[0].enabled_features.is_empty());
    assert_eq!(devices[0].iroh_public_key, "updated-key");
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

#[tokio::test]
async fn protected_routes_reject_token_for_missing_account() {
    let app = test_app().await;
    let jwt = JwtService::new("integration-test-secret");
    let token = jwt.issue(AccountId::new()).unwrap();

    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/devices/{}", DeviceId::new()))
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::to_string(&DeviceUpsertRequest {
                        device_id: DeviceId::new(),
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

    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
}

async fn login_and_get_tokens(app: &axum::Router) -> AuthResponse {
    let email = format!("refresh-{}@example.com", uuid::Uuid::new_v4());
    app.clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthRegisterRequest {
                        email: email.clone(),
                        password: "hunter22".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let login_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthLoginRequest {
                        email,
                        password: "hunter22".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(login_response.status(), axum::http::StatusCode::OK);
    let auth = body_json::<AuthResponse>(login_response.into_body()).await;
    assert!(!auth_token(&auth).is_empty());
    assert!(!refresh_token_value(&auth).is_empty());
    auth
}

#[tokio::test]
async fn refresh_rotates_tokens() {
    let app = test_app().await;
    let auth = login_and_get_tokens(&app).await;
    let refresh_token = refresh_token_value(&auth).to_owned();

    let refresh_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/refresh")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthRefreshRequest {
                        refresh_token: refresh_token.clone(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(refresh_response.status(), axum::http::StatusCode::OK);
    let refreshed: AuthResponse = body_json(refresh_response.into_body()).await;
    let new_token = auth_token(&refreshed);
    let new_refresh_token = refresh_token_value(&refreshed);
    assert!(!new_token.is_empty());
    assert!(!new_refresh_token.is_empty());
    assert_ne!(refresh_token, new_refresh_token);

    let reuse_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/refresh")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthRefreshRequest { refresh_token }).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        reuse_response.status(),
        axum::http::StatusCode::UNAUTHORIZED
    );

    let list_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/v1/devices")
                .header("authorization", format!("Bearer {new_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(list_response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn logout_revokes_refresh_token() {
    let app = test_app().await;
    let auth = login_and_get_tokens(&app).await;
    let refresh_token = refresh_token_value(&auth).to_owned();

    let logout_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/logout")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthLogoutRequest {
                        refresh_token: refresh_token.clone(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(logout_response.status(), axum::http::StatusCode::NO_CONTENT);

    let refresh_response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/refresh")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthRefreshRequest { refresh_token }).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        refresh_response.status(),
        axum::http::StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn expired_refresh_token_is_rejected() {
    let (app, db) = test_app_with_db().await;
    let auth = login_and_get_tokens(&app).await;
    let refresh_token = refresh_token_value(&auth).to_owned();

    use crate::entity::{RefreshToken, refresh_token};
    use crate::refresh_token::hash_token;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    let record = RefreshToken::find()
        .filter(refresh_token::Column::TokenHash.eq(hash_token(&refresh_token)))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let mut active: refresh_token::ActiveModel = record.into();
    active.expires_at = Set(Utc::now() - Duration::minutes(1));
    active.update(&db).await.unwrap();

    let refresh_response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/refresh")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthRefreshRequest { refresh_token }).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        refresh_response.status(),
        axum::http::StatusCode::UNAUTHORIZED
    );
}

async fn register_user(app: &axum::Router, email: &str) -> AuthResponse {
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthRegisterRequest {
                        email: email.to_owned(),
                        password: "hunter22".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    body_json(response.into_body()).await
}

#[tokio::test]
async fn totp_setup_enable_and_login_requires_mfa() {
    let app = test_app().await;
    let email = format!("mfa-{}@example.com", uuid::Uuid::new_v4());
    let auth = register_user(&app, &email).await;
    let token = auth_token(&auth);

    let setup_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/2fa/setup")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(setup_response.status(), axum::http::StatusCode::OK);
    let TotpSetupResponse { secret, otpauth_uri } =
        body_json(setup_response.into_body()).await;
    assert!(otpauth_uri.contains("otpauth://"));
    assert!(!secret.is_empty());

    let code = crate::totp::current_totp_code(&email, &secret).unwrap();
    let enable_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/2fa/enable")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&TotpEnableRequest { code }).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(enable_response.status(), axum::http::StatusCode::OK);
    let TotpEnableResponse { recovery_codes } = body_json(enable_response.into_body()).await;
    assert_eq!(recovery_codes.len(), 10);

    let login_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthLoginRequest {
                        email: email.clone(),
                        password: "hunter22".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login_response.status(), axum::http::StatusCode::OK);
    let mfa_challenge: AuthResponse = body_json(login_response.into_body()).await;
    assert!(mfa_challenge.mfa_required);
    assert!(mfa_challenge.token.is_none());
    let mfa_token = mfa_challenge
        .mfa_token
        .expect("expected mfa token");

    let mfa_code = crate::totp::current_totp_code(&email, &secret).unwrap();
    let mfa_login_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login/mfa")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthMfaLoginRequest {
                        mfa_token,
                        code: mfa_code,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mfa_login_response.status(), axum::http::StatusCode::OK);
    let session: AuthResponse = body_json(mfa_login_response.into_body()).await;
    assert!(!auth_token(&session).is_empty());
}

#[tokio::test]
async fn recovery_code_can_complete_mfa_login() {
    let app = test_app().await;
    let email = format!("recovery-{}@example.com", uuid::Uuid::new_v4());
    let auth = register_user(&app, &email).await;
    let token = auth_token(&auth);

    let setup: TotpSetupResponse = body_json(
        app.clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/2fa/setup")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .into_body(),
    )
    .await;

    let code = crate::totp::current_totp_code(&email, &setup.secret).unwrap();
    let enable: TotpEnableResponse = body_json(
        app.clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/2fa/enable")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&TotpEnableRequest { code }).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap()
            .into_body(),
    )
    .await;
    let recovery_code = enable.recovery_codes[0].clone();

    let mfa_challenge: AuthResponse = body_json(
        app.clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&AuthLoginRequest {
                            email: email.clone(),
                            password: "hunter22".to_owned(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap()
            .into_body(),
    )
    .await;

    let mfa_login_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login/mfa")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthMfaLoginRequest {
                        mfa_token: mfa_challenge.mfa_token.unwrap(),
                        code: recovery_code,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mfa_login_response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn totp_disable_turns_off_mfa() {
    let app = test_app().await;
    let email = format!("disable-{}@example.com", uuid::Uuid::new_v4());
    let auth = register_user(&app, &email).await;
    let token = auth_token(&auth);

    let setup: TotpSetupResponse = body_json(
        app.clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/2fa/setup")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .into_body(),
    )
    .await;
    let code = crate::totp::current_totp_code(&email, &setup.secret).unwrap();
    body_json::<TotpEnableResponse>(
        app.clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/2fa/enable")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&TotpEnableRequest { code }).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap()
            .into_body(),
    )
    .await;

    let disable_code = crate::totp::current_totp_code(&email, &setup.secret).unwrap();
    let disable_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/2fa/disable")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&TotpDisableRequest {
                        password: "hunter22".to_owned(),
                        code: disable_code,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(disable_response.status(), axum::http::StatusCode::NO_CONTENT);

    let login_response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthLoginRequest {
                        email,
                        password: "hunter22".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login_response.status(), axum::http::StatusCode::OK);
    let session: AuthResponse = body_json(login_response.into_body()).await;
    assert!(!session.mfa_required);
    assert!(session.token.is_some());
}

#[tokio::test]
async fn change_password_updates_credentials() {
    let app = test_app().await;
    let email = format!("password-{}@example.com", uuid::Uuid::new_v4());
    let auth = register_user(&app, &email).await;
    let token = auth_token(&auth);

    let change_response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/password")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&ChangePasswordRequest {
                        current_password: "hunter22".to_owned(),
                        new_password: "newpass99".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(change_response.status(), axum::http::StatusCode::NO_CONTENT);

    let old_login = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthLoginRequest {
                        email: email.clone(),
                        password: "hunter22".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(old_login.status(), axum::http::StatusCode::UNAUTHORIZED);

    let new_login = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&AuthLoginRequest {
                        email,
                        password: "newpass99".to_owned(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(new_login.status(), axum::http::StatusCode::OK);
}
