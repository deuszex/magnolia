//! Server identity management.
//!
//! On first startup (or when federation is first enabled) this module:
//! 1. Generates an ML-DSA-87 signing keypair.
//! 2. Persists it in the `server_identity` table, encrypting the private key
//! at rest when `ENCRYPTION_AT_REST_KEY` is set in the environment.
//!
//! On subsequent startups it loads and decrypts the existing keypair.
//!
//! The loaded keypair is held in [`ServerIdentity`] which is added to
//! `AppState` and shared across federation handlers.

use ml_dsa::{KeyGen, MlDsa87, SigningKey, VerifyingKey};
use rand_crypto::rng;
use sqlx::AnyPool;
use tracing::{info, warn};

use super::repo;
use crate::crypto::generate_keys::generate_signer_keys;
use crate::utils::encryption::ContentEncryption;

/// Byte sizes for ML-DSA-87 keys.
const VERIFYING_KEY_LEN: usize = 2592;
/// The signing key is stored as a 32-byte seed (from which the full key is derived).
const SIGNING_KEY_LEN: usize = 32;

/// AES-256-GCM nonce is prepended to the ciphertext when persisting the
/// private key encrypted. Layout: [12-byte nonce][N-byte ciphertext].
const NONCE_LEN: usize = 12;

/// Live server identity, loaded once at startup.
#[derive(Clone)]
pub struct ServerIdentity {
    /// The raw verifying (public) key bytes, shared with peers during handshake.
    /// Used by other servers to verify the signers identity.
    pub public_key_bytes: Vec<u8>,
    /// The signing key, held in memory for request signing.
    /// We keep a clone-friendly handle by re-decoding on demand; the bytes
    /// themselves are stored here to avoid holding an un-Clone type.
    signing_key_bytes: Vec<u8>,
}

impl ServerIdentity {
    /// Return the ML-DSA-87 signing key derived from the stored 32-byte seed.
    pub fn signing_key(&self) -> SigningKey<MlDsa87> {
        let mut seed = ml_dsa::Seed::default();
        seed.as_mut_slice().copy_from_slice(&self.signing_key_bytes);
        SigningKey::from_seed(&seed)
    }

    /// Return the ML-DSA-87 verifying key decoded from the stored bytes.
    pub fn verifying_key(&self) -> VerifyingKey<MlDsa87> {
        let mut arr = ml_dsa::EncodedVerifyingKey::<MlDsa87>::default();
        arr.as_mut_slice().copy_from_slice(&self.public_key_bytes);
        VerifyingKey::decode(&arr)
    }

    /// Public key as a hex string (sent in handshake payloads).
    pub fn public_key_hex(&self) -> String {
        hex::encode(&self.public_key_bytes)
    }
}

// Encryption helpers

/// Encrypt private key bytes for at-rest storage.
/// Returns `nonce || ciphertext`.
fn encrypt_private_key(enc: &ContentEncryption, key_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let (ciphertext, nonce) = enc
        .encrypt_content(key_bytes)
        .map_err(|e| format!("Failed to encrypt private key: {:?}", e))?;
    let mut blob = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

/// Decrypt private key bytes from at-rest storage.
/// Expects `nonce || ciphertext` layout.
fn decrypt_private_key(enc: &ContentEncryption, blob: &[u8]) -> Result<Vec<u8>, String> {
    if blob.len() <= NONCE_LEN {
        return Err("Private key blob too short".to_string());
    }
    let nonce = &blob[..NONCE_LEN];
    let ciphertext = &blob[NONCE_LEN..];
    enc.decrypt_content(ciphertext, nonce)
        .map_err(|e| format!("Failed to decrypt private key: {:?}", e))
}

// Public API

/// Load the server identity from the database, generating one if absent.
///
/// `enc` is `Some` when `ENCRYPTION_AT_REST_KEY` is configured.
pub async fn load_or_generate(
    pool: &AnyPool,
    enc: Option<&ContentEncryption>,
) -> Result<ServerIdentity, String> {
    match repo::get_server_identity(pool)
        .await
        .map_err(|e| format!("DB error loading server identity: {}", e))?
    {
        Some(row) => {
            info!("Loaded existing server identity from database");
            let signing_key_bytes = match enc {
                Some(cipher) => decrypt_private_key(cipher, &row.ml_dsa_private_key)?,
                None => row.ml_dsa_private_key.clone(),
            };

            if signing_key_bytes.len() != SIGNING_KEY_LEN {
                return Err(format!(
                    "Stored signing key has wrong length: {} (expected {})",
                    signing_key_bytes.len(),
                    SIGNING_KEY_LEN
                ));
            }
            if row.ml_dsa_public_key.len() != VERIFYING_KEY_LEN {
                return Err(format!(
                    "Stored verifying key has wrong length: {} (expected {})",
                    row.ml_dsa_public_key.len(),
                    VERIFYING_KEY_LEN
                ));
            }

            Ok(ServerIdentity {
                public_key_bytes: row.ml_dsa_public_key,
                signing_key_bytes,
            })
        }
        None => {
            info!("No server identity found — generating new ML-DSA-87 keypair");
            generate_and_store(pool, enc).await
        }
    }
}

async fn generate_and_store(
    pool: &AnyPool,
    enc: Option<&ContentEncryption>,
) -> Result<ServerIdentity, String> {
    let kp = generate_signer_keys();
    let verifying_key = kp.verifying_key();

    let public_key_bytes = verifying_key.encode().as_slice().to_vec();
    let signing_key_bytes = kp.to_seed().as_slice().to_vec();

    let stored_private = match enc {
        Some(cipher) => encrypt_private_key(cipher, &signing_key_bytes)?,
        None => {
            warn!(
                "ENCRYPTION_AT_REST_KEY not set — server ML-DSA private key \
 will be stored unencrypted. Set this variable in production."
            );
            signing_key_bytes.clone()
        }
    };

    repo::insert_server_identity(pool, &public_key_bytes, &stored_private)
        .await
        .map_err(|e| format!("Failed to persist server identity: {}", e))?;

    info!("Generated and persisted new server ML-DSA-87 identity");

    Ok(ServerIdentity {
        public_key_bytes,
        signing_key_bytes,
    })
}

/// Encode the shared secret for at-rest storage.
/// Uses the same nonce-prefix layout as the private key.
pub fn encrypt_shared_secret(enc: &ContentEncryption, secret: &[u8]) -> Result<Vec<u8>, String> {
    encrypt_private_key(enc, secret)
}

/// Decode a stored shared secret.
pub fn decrypt_shared_secret(enc: &ContentEncryption, blob: &[u8]) -> Result<Vec<u8>, String> {
    decrypt_private_key(enc, blob)
}

/// Encode a shared secret when no at-rest encryption is configured.
/// Returns the raw bytes unchanged (caller must ensure the DB file itself is protected).
pub fn store_shared_secret_raw(secret: &[u8]) -> Vec<u8> {
    secret.to_vec()
}

/// Return the shared secret bytes from the DB blob, decrypting if needed.
pub fn load_shared_secret(blob: &[u8], enc: Option<&ContentEncryption>) -> Result<Vec<u8>, String> {
    match enc {
        Some(cipher) => decrypt_shared_secret(cipher, blob),
        None => Ok(blob.to_vec()),
    }
}
