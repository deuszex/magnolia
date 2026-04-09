use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
use magnolia_common::errors::AppError;
use rand::RngCore;

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
