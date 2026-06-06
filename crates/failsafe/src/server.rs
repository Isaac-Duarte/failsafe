use std::path::PathBuf;
use std::sync::Arc;

use failsafe_core::api::{
    AuthLoginRequest, AuthRefreshRequest, AuthRegisterRequest, AuthResponse, DeviceInfo,
    DeviceListResponse, DevicePatchRequest, DeviceUpsertRequest, PairingCreateResponse,
    PairingRedeemRequest,
};
use failsafe_core::device::DeviceId;
use reqwest::RequestBuilder;
use tokio::sync::Mutex;

use crate::credentials::Credentials;
use crate::error::DaemonError;

#[derive(Clone)]
pub struct ServerClient {
    base_url: String,
    credentials: Arc<Mutex<Credentials>>,
    credentials_path: Option<PathBuf>,
    http: reqwest::Client,
}

impl ServerClient {
    pub fn new(
        base_url: String,
        credentials: Credentials,
        credentials_path: Option<PathBuf>,
    ) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            credentials: Arc::new(Mutex::new(credentials)),
            credentials_path,
            http: reqwest::Client::new(),
        }
    }

    pub async fn register(
        base_url: &str,
        email: &str,
        password: &str,
    ) -> Result<AuthResponse, DaemonError> {
        let client = reqwest::Client::new();
        let url = format!("{}/api/v1/auth/register", base_url.trim_end_matches('/'));
        let response = client
            .post(url)
            .json(&AuthRegisterRequest {
                email: email.to_owned(),
                password: password.to_owned(),
            })
            .send()
            .await
            .map_err(|error| DaemonError::Config(format!("register request failed: {error}")))?;

        parse_json_response(response).await
    }

    pub async fn login(
        base_url: &str,
        email: &str,
        password: &str,
    ) -> Result<AuthResponse, DaemonError> {
        let client = reqwest::Client::new();
        let url = format!("{}/api/v1/auth/login", base_url.trim_end_matches('/'));
        let response = client
            .post(url)
            .json(&AuthLoginRequest {
                email: email.to_owned(),
                password: password.to_owned(),
            })
            .send()
            .await
            .map_err(|error| DaemonError::Config(format!("login request failed: {error}")))?;

        parse_json_response(response).await
    }

    pub async fn create_pairing_code(&self) -> Result<PairingCreateResponse, DaemonError> {
        let response = self
            .send_authenticated(|token| {
                self.http
                    .post(format!("{}/api/v1/pairing", self.base_url))
                    .bearer_auth(token)
                    .json(&serde_json::json!({}))
            })
            .await
            .map_err(|error| DaemonError::Config(format!("create pairing code failed: {error}")))?;

        parse_json_response(response).await
    }

    pub async fn redeem_pairing_code(
        base_url: &str,
        code: &str,
        device: Option<DeviceUpsertRequest>,
    ) -> Result<AuthResponse, DaemonError> {
        let client = reqwest::Client::new();
        let url = format!("{}/api/v1/pairing/redeem", base_url.trim_end_matches('/'));
        let response = client
            .post(url)
            .json(&PairingRedeemRequest {
                code: code.to_owned(),
                device,
            })
            .send()
            .await
            .map_err(|error| DaemonError::Config(format!("redeem pairing code failed: {error}")))?;

        parse_json_response(response).await
    }

    pub async fn upsert_device(&self, request: DeviceUpsertRequest) -> Result<(), DaemonError> {
        let device_id = request.device_id;
        let response = self
            .send_authenticated(|token| {
                self.http
                    .put(format!("{}/api/v1/devices/{device_id}", self.base_url))
                    .bearer_auth(token)
                    .json(&request)
            })
            .await
            .map_err(|error| DaemonError::Config(format!("device upsert failed: {error}")))?;

        map_device_mutation_response(response, "device upsert").await
    }

    pub async fn heartbeat_device(&self, device_id: DeviceId) -> Result<(), DaemonError> {
        let response = self
            .send_authenticated(|token| {
                self.http
                    .post(format!(
                        "{}/api/v1/devices/{device_id}/heartbeat",
                        self.base_url
                    ))
                    .bearer_auth(token)
            })
            .await
            .map_err(|error| DaemonError::Config(format!("device heartbeat failed: {error}")))?;

        map_device_mutation_response(response, "device heartbeat").await
    }

    pub async fn patch_device(
        &self,
        device_id: DeviceId,
        request: DevicePatchRequest,
    ) -> Result<DeviceInfo, DaemonError> {
        let response = self
            .send_authenticated(|token| {
                self.http
                    .patch(format!("{}/api/v1/devices/{device_id}", self.base_url))
                    .bearer_auth(token)
                    .json(&request)
            })
            .await
            .map_err(|error| DaemonError::Config(format!("device patch failed: {error}")))?;

        parse_json_response(response).await
    }

    pub async fn delete_device(&self, device_id: DeviceId) -> Result<(), DaemonError> {
        let response = self
            .send_authenticated(|token| {
                self.http
                    .delete(format!("{}/api/v1/devices/{device_id}", self.base_url))
                    .bearer_auth(token)
            })
            .await
            .map_err(|error| DaemonError::Config(format!("device delete failed: {error}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DaemonError::Config(format!(
                "device delete returned {status}: {body}"
            )));
        }

        Ok(())
    }

    pub async fn list_devices(&self) -> Result<DeviceListResponse, DaemonError> {
        let response = self
            .send_authenticated(|token| {
                self.http
                    .get(format!("{}/api/v1/devices", self.base_url))
                    .bearer_auth(token)
            })
            .await
            .map_err(|error| DaemonError::Config(format!("list devices failed: {error}")))?;

        parse_json_response(response).await
    }

    async fn send_authenticated(
        &self,
        mut build: impl FnMut(&str) -> RequestBuilder,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let token = self.auth_token().await;
        let response = build(&token).send().await?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            && self.refresh_tokens().await.unwrap_or(false)
        {
            let token = self.auth_token().await;
            return build(&token).send().await;
        }

        Ok(response)
    }

    async fn auth_token(&self) -> String {
        self.credentials.lock().await.auth_token.clone()
    }

    async fn refresh_tokens(&self) -> Result<bool, DaemonError> {
        let refresh_token = {
            let credentials = self.credentials.lock().await;
            credentials.refresh_token.clone()
        };

        let Some(refresh_token) = refresh_token else {
            return Ok(false);
        };

        let url = format!("{}/api/v1/auth/refresh", self.base_url);
        let response = self
            .http
            .post(url)
            .json(&AuthRefreshRequest { refresh_token })
            .send()
            .await
            .map_err(|error| DaemonError::Config(format!("token refresh failed: {error}")))?;

        if !response.status().is_success() {
            return Ok(false);
        }

        let body = response.json::<AuthResponse>().await.map_err(|error| {
            DaemonError::Config(format!("failed to decode refresh response: {error}"))
        })?;

        let token = body
            .token
            .ok_or_else(|| DaemonError::Config("refresh response missing token".to_owned()))?;
        let refresh_token = body.refresh_token.ok_or_else(|| {
            DaemonError::Config("refresh response missing refresh_token".to_owned())
        })?;

        let mut credentials = self.credentials.lock().await;
        credentials.auth_token = token;
        credentials.refresh_token = Some(refresh_token);

        if let Some(path) = &self.credentials_path {
            credentials.save(path)?;
        }

        Ok(true)
    }
}

async fn map_device_mutation_response(
    response: reqwest::Response,
    action: &str,
) -> Result<(), DaemonError> {
    if response.status() == reqwest::StatusCode::FORBIDDEN {
        let body = response.text().await.unwrap_or_default();
        if body.contains("device removed") {
            return Err(DaemonError::DeviceRemoved);
        }
        return Err(DaemonError::Config(format!(
            "{action} returned 403: {body}"
        )));
    }

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(DaemonError::Config(format!(
            "{action} returned {status}: {body}"
        )));
    }

    Ok(())
}

async fn parse_json_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, DaemonError> {
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(DaemonError::Config(format!(
            "server returned {status}: {body}"
        )));
    }

    response
        .json::<T>()
        .await
        .map_err(|error| DaemonError::Config(format!("failed to decode server response: {error}")))
}
