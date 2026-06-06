use std::time::{Duration, SystemTime, UNIX_EPOCH};

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use failsafe_core::api::AccountId;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use crate::error::{AuthError, ServerResult};

const TOKEN_TTL: Duration = Duration::from_secs(60 * 60);
const MFA_TOKEN_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
    iat: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mfa_pending: Option<bool>,
}

#[derive(Clone)]
pub struct JwtService {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl JwtService {
    pub fn new(secret: &str) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
        }
    }

    pub fn issue(&self, account_id: AccountId) -> ServerResult<String> {
        self.issue_with_ttl(account_id, TOKEN_TTL, None)
    }

    pub fn issue_mfa_pending(&self, account_id: AccountId) -> ServerResult<String> {
        self.issue_with_ttl(account_id, MFA_TOKEN_TTL, Some(true))
    }

    fn issue_with_ttl(
        &self,
        account_id: AccountId,
        ttl: Duration,
        mfa_pending: Option<bool>,
    ) -> ServerResult<String> {
        let now = unix_now();
        let claims = Claims {
            sub: account_id.to_string(),
            iat: now,
            exp: now + ttl.as_secs() as usize,
            mfa_pending,
        };

        encode(&Header::default(), &claims, &self.encoding)
            .map_err(|error| AuthError::Jwt(error.to_string()).into())
    }

    pub fn validate(&self, token: &str) -> ServerResult<AccountId> {
        let data = decode::<Claims>(token, &self.decoding, &Validation::default())
            .map_err(|_| AuthError::InvalidToken)?;

        if data.claims.mfa_pending == Some(true) {
            return Err(AuthError::InvalidToken.into());
        }

        data.claims
            .sub
            .parse::<AccountId>()
            .map_err(|_| AuthError::InvalidToken.into())
    }

    pub fn validate_mfa_pending(&self, token: &str) -> ServerResult<AccountId> {
        let data = decode::<Claims>(token, &self.decoding, &Validation::default())
            .map_err(|_| AuthError::InvalidToken)?;

        if data.claims.mfa_pending != Some(true) {
            return Err(AuthError::InvalidToken.into());
        }

        data.claims
            .sub
            .parse::<AccountId>()
            .map_err(|_| AuthError::InvalidToken.into())
    }
}

pub fn hash_password(password: &str) -> ServerResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| AuthError::HashingFailed.into())
}

pub fn verify_password(password: &str, password_hash: &str) -> ServerResult<()> {
    let parsed = PasswordHash::new(password_hash).map_err(|_| AuthError::InvalidCredentials)?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| AuthError::InvalidCredentials.into())
}

fn unix_now() -> usize {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_secs() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_roundtrip() {
        let hash = hash_password("hunter2").unwrap();
        verify_password("hunter2", &hash).unwrap();
        assert!(verify_password("wrong", &hash).is_err());
    }

    #[test]
    fn jwt_issue_and_validate() {
        let service = JwtService::new("test-secret");
        let account_id = AccountId::new();
        let token = service.issue(account_id).unwrap();
        assert_eq!(service.validate(&token).unwrap(), account_id);
    }
}
