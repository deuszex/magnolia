use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
use magnolia_common::errors::AppError;
use rand::RngCore;

pub fn encrypt(
    aes: &Aes256Gcm,
    nonce: [u8; 12],
    content: &[u8],
) -> Result<Vec<u8>, aes_gcm::Error> {
    aes.encrypt((&nonce).into(), content)
}

pub fn decrypt(
    aes: &Aes256Gcm,
    nonce: [u8; 12],
    encrypted: &[u8],
) -> Result<Vec<u8>, aes_gcm::Error> {
    aes.decrypt((&nonce).into(), encrypted)
}

/// High-level content encryption service for server-side encryption at rest.
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

    /// Generate a random 12-byte nonce.
    fn generate_nonce() -> [u8; 12] {
        let mut nonce = [0u8; 12];
        rand::rng().fill_bytes(&mut nonce);
        nonce
    }

    /// Encrypt content, returning (encrypted_data, nonce).
    pub fn encrypt_content(&self, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), AppError> {
        let nonce = Self::generate_nonce();

        let encrypted = encrypt(&self.cipher, nonce, plaintext)
            .map_err(|e| AppError::Internal(format!("Encryption failed: {}", e)))?;

        Ok((encrypted, nonce.to_vec()))
    }

    /// Decrypt content using the stored nonce.
    pub fn decrypt_content(&self, encrypted: &[u8], nonce: &[u8]) -> Result<Vec<u8>, AppError> {
        let nonce_arr: [u8; 12] = nonce
            .try_into()
            .map_err(|_| AppError::Internal("Invalid nonce length".to_string()))?;

        decrypt(&self.cipher, nonce_arr, encrypted)
            .map_err(|e| AppError::Internal(format!("Decryption failed: {}", e)))
    }

    /// Encrypt a string, returning (encrypted_data, nonce).
    pub fn encrypt_string(&self, plaintext: &str) -> Result<(Vec<u8>, Vec<u8>), AppError> {
        self.encrypt_content(plaintext.as_bytes())
    }

    /// Decrypt to a UTF-8 string.
    pub fn decrypt_to_string(&self, encrypted: &[u8], nonce: &[u8]) -> Result<String, AppError> {
        let bytes = self.decrypt_content(encrypted, nonce)?;
        String::from_utf8(bytes)
            .map_err(|e| AppError::Internal(format!("Decrypted content is not valid UTF-8: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> String {
        // 32 bytes in hex = 64 hex chars
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string()
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let enc = ContentEncryption::from_hex_key(&test_key()).unwrap();
        let plaintext = b"Hello, world!";

        let (encrypted, nonce) = enc.encrypt_content(plaintext).unwrap();
        assert_ne!(encrypted, plaintext);

        let decrypted = enc.decrypt_content(&encrypted, &nonce).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_string() {
        let enc = ContentEncryption::from_hex_key(&test_key()).unwrap();
        let plaintext = "This is a test message with UTF-8: café ñ 你好";

        let (encrypted, nonce) = enc.encrypt_string(plaintext).unwrap();
        let decrypted = enc.decrypt_to_string(&encrypted, &nonce).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_nonces_each_time() {
        let enc = ContentEncryption::from_hex_key(&test_key()).unwrap();
        let plaintext = b"Same content";

        let (_, nonce1) = enc.encrypt_content(plaintext).unwrap();
        let (_, nonce2) = enc.encrypt_content(plaintext).unwrap();
        assert_ne!(nonce1, nonce2);
    }

    #[test]
    fn test_invalid_key_length() {
        let result = ContentEncryption::from_hex_key("aabbcc");
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_nonce_fails() {
        let enc = ContentEncryption::from_hex_key(&test_key()).unwrap();
        let (encrypted, _) = enc.encrypt_content(b"secret").unwrap();
        let wrong_nonce = vec![0u8; 12];
        let result = enc.decrypt_content(&encrypted, &wrong_nonce);
        assert!(result.is_err());
    }
}
