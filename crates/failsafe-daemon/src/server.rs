use failsafe_core::api::{
    AuthLoginRequest, AuthRegisterRequest, AuthResponse, DeviceListResponse, DeviceUpsertRequest,
};

use crate::error::DaemonError;

#[derive(Clone)]
pub struct ServerClient {
    base_url: String,
    auth_token: String,
    http: reqwest::Client,
}

impl ServerClient {
    pub fn new(base_url: String, auth_token: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            auth_token,
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

    pub async fn upsert_device(&self, request: DeviceUpsertRequest) -> Result<(), DaemonError> {
        let url = format!(
            "{}/api/v1/devices/{}",
            self.base_url, request.device_id
        );
        let response = self
            .http
            .put(url)
            .bearer_auth(&self.auth_token)
            .json(&request)
            .send()
            .await
            .map_err(|error| DaemonError::Config(format!("device upsert failed: {error}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DaemonError::Config(format!(
                "device upsert returned {status}: {body}"
            )));
        }

        Ok(())
    }

    pub async fn list_devices(&self) -> Result<DeviceListResponse, DaemonError> {
        let url = format!("{}/api/v1/devices", self.base_url);
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.auth_token)
            .send()
            .await
            .map_err(|error| DaemonError::Config(format!("list devices failed: {error}")))?;

        parse_json_response(response).await
    }
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
