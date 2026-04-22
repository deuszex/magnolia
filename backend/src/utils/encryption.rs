use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
use magnolia_common::errors::AppError;
use magnolia_common::repositories::SiteConfigRepository;
use rand::RngCore;
use sqlx::AnyPool;

use crate::config::Settings;

/// AES-256-GCM content encryption service for encryption at rest.
#[derive(Clone)]
pub struct ContentEncryption {
    cipher: Aes256Gcm,
}

impl ContentEncryption {
    /// Create from a hex-encoded 32-byte key string.
    pub fn from_hex_key(hex_key: &str) -> Result<Self, AppError> {
        let key_bytes = hex::decode(hex_key)
            .map_err(|e| AppError::Internal(format!("Invalid encryption key hex: {}", e)))?;

        if key_bytes.len() != 32 {
            return Err(AppError::Internal(format!(
                "Encryption key must be 32 bytes, got {}",
                key_bytes.len()
            )));
        }

        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| AppError::Internal(format!("Failed to create cipher: {}", e)))?;

        Ok(Self { cipher })
    }

    fn generate_nonce() -> [u8; 12] {
        let mut nonce = [0u8; 12];
        rand::rng().fill_bytes(&mut nonce);
        nonce
    }

    /// Encrypt content, returning (encrypted_data, nonce).
    pub fn encrypt_content(&self, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), AppError> {
        let nonce = Self::generate_nonce();
        let encrypted = self
            .cipher
            .encrypt((&nonce).into(), plaintext)
            .map_err(|e| AppError::Internal(format!("Encryption failed: {}", e)))?;
        Ok((encrypted, nonce.to_vec()))
    }

    /// Decrypt content using the stored nonce.
    pub fn decrypt_content(&self, encrypted: &[u8], nonce: &[u8]) -> Result<Vec<u8>, AppError> {
        let nonce_arr: [u8; 12] = nonce
            .try_into()
            .map_err(|_| AppError::Internal("Invalid nonce length".to_string()))?;
        self.cipher
            .decrypt((&nonce_arr).into(), encrypted)
            .map_err(|e| AppError::Internal(format!("Decryption failed: {}", e)))
    }
}

/// Encrypt a text string for storage if encryption at rest is enabled.
/// Returns `(stored_content, Some(nonce_hex))` when encrypted, or `(original, None)` otherwise.
pub async fn encrypt_text(
    settings: &Settings,
    pool: &AnyPool,
    text: &str,
) -> Result<(String, Option<String>), AppError> {
    let key = match settings.encryption_at_rest_key.as_deref() {
        Some(k) => k,
        None => return Ok((text.to_string(), None)),
    };
    let config = SiteConfigRepository::new(pool.clone()).get().await.ok();
    if !config
        .map(|c| c.encryption_at_rest_enabled == 1)
        .unwrap_or(false)
    {
        return Ok((text.to_string(), None));
    }
    let enc = ContentEncryption::from_hex_key(key)?;
    let (ciphertext, nonce) = enc.encrypt_content(text.as_bytes())?;
    Ok((hex::encode(ciphertext), Some(hex::encode(nonce))))
}

/// Decrypt a text string that may have been encrypted at rest.
/// If `nonce_hex` is None, the content is returned as-is (stored as plaintext).
pub fn decrypt_text(
    settings: &Settings,
    content: &str,
    nonce_hex: &Option<String>,
) -> Result<String, AppError> {
    let nonce = match nonce_hex {
        Some(n) => n,
        None => return Ok(content.to_string()),
    };
    let key = settings.encryption_at_rest_key.as_deref().ok_or_else(|| {
        AppError::Internal("Encrypted content but ENCRYPTION_AT_REST_KEY not set".to_string())
    })?;
    let enc = ContentEncryption::from_hex_key(key)?;
    let ciphertext = hex::decode(content)
        .map_err(|_| AppError::Internal("Invalid stored ciphertext".to_string()))?;
    let nonce_bytes =
        hex::decode(nonce).map_err(|_| AppError::Internal("Invalid stored nonce".to_string()))?;
    let plaintext = enc.decrypt_content(&ciphertext, &nonce_bytes)?;
    String::from_utf8(plaintext)
        .map_err(|_| AppError::Internal("Decrypted content is not valid UTF-8".to_string()))
}
