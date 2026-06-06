use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use totp_rs::{Algorithm, Secret, TOTP};

use crate::error::{AuthError, ServerError, ServerResult};

const RECOVERY_CODE_COUNT: usize = 10;
const RECOVERY_CODE_LENGTH: usize = 8;

pub fn hash_recovery_code(code: &str) -> String {
    let hash = Sha256::digest(code.trim().to_uppercase().as_bytes());
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn generate_recovery_codes() -> Vec<String> {
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = OsRng;

    (0..RECOVERY_CODE_COUNT)
        .map(|_| {
            (0..RECOVERY_CODE_LENGTH)
                .map(|_| {
                    let idx = (rng.next_u32() as usize) % CHARSET.len();
                    CHARSET[idx] as char
                })
                .collect()
        })
        .collect()
}

pub fn derive_encryption_key(secret: &str) -> [u8; 32] {
    let hash = Sha256::digest(secret.as_bytes());
    hash.into()
}

pub fn encrypt_secret(encryption_key: &str, plaintext: &str) -> ServerResult<String> {
    let key = derive_encryption_key(encryption_key);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| AuthError::HashingFailed)?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| AuthError::HashingFailed)?;
    Ok(format!(
        "{}:{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, nonce_bytes),
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, ciphertext)
    ))
}

pub fn decrypt_secret(encryption_key: &str, stored: &str) -> ServerResult<String> {
    let (nonce_b64, ciphertext_b64) = stored.split_once(':').ok_or(ServerError::Internal(
        "invalid encrypted secret format".to_owned(),
    ))?;
    let nonce_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, nonce_b64)
        .map_err(|_| ServerError::Internal("invalid encrypted secret nonce".to_owned()))?;
    let ciphertext =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, ciphertext_b64)
            .map_err(|_| ServerError::Internal("invalid encrypted secret ciphertext".to_owned()))?;
    let key = derive_encryption_key(encryption_key);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| AuthError::HashingFailed)?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| AuthError::InvalidCredentials)?;
    String::from_utf8(plaintext).map_err(|_| AuthError::InvalidCredentials.into())
}

pub fn generate_totp_secret() -> String {
    Secret::generate_secret().to_encoded().to_string()
}

pub fn build_otpauth_uri(email: &str, secret: &str) -> ServerResult<String> {
    let totp = build_totp(email, secret)?;
    Ok(totp.get_url())
}

fn build_totp(email: &str, secret: &str) -> ServerResult<TOTP> {
    TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        Secret::Encoded(secret.to_owned())
            .to_bytes()
            .map_err(|_| AuthError::InvalidCredentials)?,
        Some("Failsafe".to_owned()),
        email.to_owned(),
    )
    .map_err(|_| AuthError::InvalidCredentials.into())
}

pub fn verify_totp(email: &str, secret: &str, code: &str) -> ServerResult<()> {
    let totp = build_totp(email, secret)?;
    let trimmed = code.trim();
    if trimmed.len() != 6 || !trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(AuthError::InvalidCredentials.into());
    }

    if totp
        .check_current(trimmed)
        .map_err(|_| AuthError::InvalidCredentials)?
    {
        Ok(())
    } else {
        Err(AuthError::InvalidCredentials.into())
    }
}

pub fn current_totp_code(email: &str, secret: &str) -> ServerResult<String> {
    let totp = build_totp(email, secret)?;
    totp.generate_current()
        .map_err(|_| AuthError::InvalidCredentials.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let secret = generate_totp_secret();
        let encrypted = encrypt_secret("test-key", &secret).unwrap();
        let decrypted = decrypt_secret("test-key", &encrypted).unwrap();
        assert_eq!(secret, decrypted);
    }

    #[test]
    fn totp_verify_current_code() {
        let secret = generate_totp_secret();
        let email = "user@example.com";
        let code = current_totp_code(email, &secret).unwrap();
        verify_totp(email, &secret, &code).unwrap();
        assert!(verify_totp(email, &secret, "000000").is_err());
    }
}
