use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::device::DeviceId;
use crate::feature::FeatureId;

/// Account identifier used in JWT claims and server storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountId(pub Uuid);

impl AccountId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AccountId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for AccountId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct AuthRegisterRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct AuthLoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct AuthResponse {
    pub token: Option<String>,
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub mfa_required: bool,
    pub mfa_token: Option<String>,
}

impl AuthResponse {
    pub fn authenticated(token: String, refresh_token: String) -> Self {
        Self {
            token: Some(token),
            refresh_token: Some(refresh_token),
            mfa_required: false,
            mfa_token: None,
        }
    }

    pub fn mfa_required(mfa_token: String) -> Self {
        Self {
            token: None,
            refresh_token: None,
            mfa_required: true,
            mfa_token: Some(mfa_token),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct AuthMfaLoginRequest {
    pub mfa_token: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct AuthRefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct AuthLogoutRequest {
    pub refresh_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct AccountResponse {
    pub email: String,
    pub totp_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct TotpSetupResponse {
    pub otpauth_uri: String,
    pub secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct TotpEnableRequest {
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct TotpEnableResponse {
    pub recovery_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct TotpDisableRequest {
    pub password: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// Create or update a device's transport registration.
///
/// On create, all fields are stored. On update, the server only applies
/// `iroh_public_key` and `last_seen`; use [`DevicePatchRequest`] to change
/// `name` or `enabled_features`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct DeviceUpsertRequest {
    pub device_id: DeviceId,
    pub name: String,
    pub iroh_public_key: String,
    pub enabled_features: Vec<FeatureId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct DeviceInfo {
    pub device_id: DeviceId,
    pub name: String,
    pub iroh_public_key: String,
    pub enabled_features: Vec<FeatureId>,
    pub last_seen: Option<String>,
    pub online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct DeviceListResponse {
    pub devices: Vec<DeviceInfo>,
}

/// Update server-managed device policy (`name`, `enabled_features`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct DevicePatchRequest {
    pub name: Option<String>,
    pub enabled_features: Option<Vec<FeatureId>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct PairingCreateResponse {
    pub code: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct PairingRedeemRequest {
    pub code: String,
    pub device: Option<DeviceUpsertRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub struct ApiError {
    pub error: String,
}
