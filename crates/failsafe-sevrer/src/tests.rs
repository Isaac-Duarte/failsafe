use axum::body::Body;
use failsafe_core::api::{
    AuthLoginRequest, AuthRegisterRequest, AuthResponse, DeviceListResponse, DeviceUpsertRequest,
    PairingCreateResponse, PairingRedeemRequest,
};
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::{build_app, AppState};
use crate::auth::JwtService;
use crate::connect_and_migrate;

async fn body_json<T: serde::de::DeserializeOwned>(body: Body) -> T {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn test_app() -> axum::Router {
    let db = connect_and_migrate("sqlite::memory:")
        .await
        .expect("in-memory database should initialize");
    let state = AppState {
        db,
        jwt: JwtService::new("integration-test-secret"),
    };
    build_app(state)
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
    assert!(code.chars().all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit()));
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
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(redeem_response.status(), axum::http::StatusCode::OK);
    let AuthResponse { token: redeemed_token } = body_json(redeem_response.into_body()).await;
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
