use failsafe_core::api::{
    AuthLoginRequest, AuthRegisterRequest, AuthResponse, DeviceInfo, DeviceListResponse,
    DevicePatchRequest, DeviceUpsertRequest, PairingCreateResponse, PairingRedeemRequest,
};
use failsafe_core::device::DeviceId;

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

    pub async fn create_pairing_code(&self) -> Result<PairingCreateResponse, DaemonError> {
        let url = format!("{}/api/v1/pairing", self.base_url);
        let response = self
            .http
            .post(url)
            .bearer_auth(&self.auth_token)
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|error| DaemonError::Config(format!("create pairing code failed: {error}")))?;

        parse_json_response(response).await
    }

    pub async fn redeem_pairing_code(
        base_url: &str,
        code: &str,
    ) -> Result<AuthResponse, DaemonError> {
        let client = reqwest::Client::new();
        let url = format!("{}/api/v1/pairing/redeem", base_url.trim_end_matches('/'));
        let response = client
            .post(url)
            .json(&PairingRedeemRequest {
                code: code.to_owned(),
            })
            .send()
            .await
            .map_err(|error| DaemonError::Config(format!("redeem pairing code failed: {error}")))?;

        parse_json_response(response).await
    }

    pub async fn upsert_device(&self, request: DeviceUpsertRequest) -> Result<(), DaemonError> {
        let url = format!("{}/api/v1/devices/{}", self.base_url, request.device_id);
        let response = self
            .http
            .put(url)
            .bearer_auth(&self.auth_token)
            .json(&request)
            .send()
            .await
            .map_err(|error| DaemonError::Config(format!("device upsert failed: {error}")))?;

        map_device_mutation_response(response, "device upsert").await
    }

    pub async fn heartbeat_device(&self, device_id: DeviceId) -> Result<(), DaemonError> {
        let url = format!("{}/api/v1/devices/{}/heartbeat", self.base_url, device_id);
        let response = self
            .http
            .post(url)
            .bearer_auth(&self.auth_token)
            .send()
            .await
            .map_err(|error| DaemonError::Config(format!("device heartbeat failed: {error}")))?;

        map_device_mutation_response(response, "device heartbeat").await
    }

    pub async fn patch_device(
        &self,
        device_id: DeviceId,
        request: DevicePatchRequest,
    ) -> Result<DeviceInfo, DaemonError> {
        let url = format!("{}/api/v1/devices/{}", self.base_url, device_id);
        let response = self
            .http
            .patch(url)
            .bearer_auth(&self.auth_token)
            .json(&request)
            .send()
            .await
            .map_err(|error| DaemonError::Config(format!("device patch failed: {error}")))?;

        parse_json_response(response).await
    }

    pub async fn delete_device(&self, device_id: DeviceId) -> Result<(), DaemonError> {
        let url = format!("{}/api/v1/devices/{}", self.base_url, device_id);
        let response = self
            .http
            .delete(url)
            .bearer_auth(&self.auth_token)
            .send()
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
