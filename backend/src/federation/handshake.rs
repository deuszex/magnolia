//! ML-KEM-1024 handshake logic.
//!
//! ## Protocol
//!
//! **Side A (initiator):**
//! 1. Generate ephemeral ML-KEM keypair `(dk_a, ek_a)`.
//! 2. POST `/api/s2s/connect` to Side B with own address, ML-DSA public key,
//! and `ek_a` (hex-encoded). The request is signed with Side A's ML-DSA key.
//! 3. Store `dk_a` temporarily until Side B responds.
//! 4. On accept: receive `ciphertext` from Side B, call `decapsulate(dk_a, ct)`
//! → 32-byte shared secret. Discard `dk_a`.
//!
//! **Side B (acceptor):**
//! 1. Receive the request. Display to admin.
//! 2. Admin approves: call `encapsulate(ek_a)` → `(ct, ss)`.
//! 3. POST `/api/s2s/connect/accept` to Side A with own address, ML-DSA public key,
//! and `ct` (hex-encoded). Request signed with Side B's ML-DSA key.
//! 4. Both sides now hold `ss`. Store it encrypted at rest.
//!
//! The shared secret is 32 bytes (ML-KEM-1024 output), used directly as the
//! AES-256-GCM key for server-level message encryption.

use ml_kem::{
    B32, KeyExport, MlKem1024,
    array::Array,
    kem::{Decapsulate, DecapsulationKey, EncapsulationKey},
};
use rand_crypto::{Rng, rng};

use crate::crypto::generate_keys::{generate_exchange_keys, key_exchange};

/// Generate an ephemeral ML-KEM-1024 keypair for Side A to use in a new
/// connection request.
///
/// Returns `(decapsulation_key_bytes, encapsulation_key_hex)`.
pub fn generate_handshake_keys() -> (Vec<u8>, String) {
    let (dk, ek) = generate_exchange_keys();
    let dk_bytes = dk.to_bytes().as_slice().to_vec();
    let ek_hex = hex::encode(ek.to_bytes());
    (dk_bytes, ek_hex)
}

/// Side B: encapsulate a shared secret using Side A's encapsulation key.
///
/// `ek_hex` is the hex-encoded encapsulation key received from Side A.
/// Returns `(ciphertext_hex, shared_secret_32_bytes)`.
pub fn encapsulate(ek_hex: &str) -> Result<(String, Vec<u8>), String> {
    let ek_bytes =
        hex::decode(ek_hex).map_err(|e| format!("Invalid encapsulation key hex: {}", e))?;

    let mut ek_arr = Array::default();
    if ek_bytes.len() != ek_arr.len() {
        return Err(format!(
            "Encapsulation key wrong length: {} (expected {})",
            ek_bytes.len(),
            ek_arr.len()
        ));
    }
    ek_arr.as_mut_slice().copy_from_slice(&ek_bytes);
    let ek = EncapsulationKey::<MlKem1024>::new(&ek_arr)
        .map_err(|_| "Invalid encapsulation key".to_string())?;

    // Generate a random 32-byte seed for deterministic encapsulation.
    let mut rng_inst = rng();
    let mut shared_seed = B32::default();
    rng_inst.fill_bytes(shared_seed.as_mut_slice());

    let (ct, ss) = ek.encapsulate_deterministic(&shared_seed);

    let ct_hex = hex::encode(ct.as_slice());
    let ss_bytes = ss.as_slice().to_vec();

    Ok((ct_hex, ss_bytes))
}

/// Side A: decapsulate the ciphertext from Side B to recover the shared secret.
///
/// `dk_bytes` is the raw decapsulation key stored from `generate_handshake_keys`.
/// `ct_hex` is the hex-encoded ciphertext received from Side B.
/// Returns the 32-byte shared secret.
pub fn decapsulate(dk_bytes: &[u8], ct_hex: &str) -> Result<Vec<u8>, String> {
    let ct_bytes = hex::decode(ct_hex).map_err(|e| format!("Invalid ciphertext hex: {}", e))?;
    let ss = key_exchange(&decode_decapsulation_key(dk_bytes)?, ct_bytes);
    Ok(ss)
}

/// Decode raw bytes back into an ML-KEM-1024 decapsulation key.
fn decode_decapsulation_key(bytes: &[u8]) -> Result<DecapsulationKey<MlKem1024>, String> {
    let mut arr = ml_kem::Seed::default();
    if bytes.len() != arr.len() {
        return Err(format!(
            "Decapsulation key wrong length: {} (expected {})",
            bytes.len(),
            arr.len()
        ));
    }
    arr.as_mut_slice().copy_from_slice(bytes);
    Ok(DecapsulationKey::<MlKem1024>::from_seed(arr))
}

/// Encrypt a message payload using the shared AES-256-GCM key.
///
/// Returns `(ciphertext_base64, nonce_hex)`.
pub fn encrypt_s2s_payload(
    shared_secret: &[u8],
    plaintext: &[u8],
) -> Result<(String, String), String> {
    use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
    use rand::RngCore;

    if shared_secret.len() != 32 {
        return Err(format!(
            "Shared secret must be 32 bytes, got {}",
            shared_secret.len()
        ));
    }

    let cipher = Aes256Gcm::new_from_slice(shared_secret)
        .map_err(|e| format!("AES key init failed: {}", e))?;

    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill_bytes(&mut nonce_bytes);

    let ciphertext = cipher
        .encrypt((&nonce_bytes).into(), plaintext)
        .map_err(|e| format!("AES-GCM encrypt failed: {}", e))?;

    Ok((
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &ciphertext),
        hex::encode(nonce_bytes),
    ))
}

/// Decrypt a message payload using the shared AES-256-GCM key.
pub fn decrypt_s2s_payload(
    shared_secret: &[u8],
    ciphertext_b64: &str,
    nonce_hex: &str,
) -> Result<Vec<u8>, String> {
    use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
    use base64::Engine;

    if shared_secret.len() != 32 {
        return Err(format!(
            "Shared secret must be 32 bytes, got {}",
            shared_secret.len()
        ));
    }

    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(ciphertext_b64)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;

    let nonce_bytes =
        hex::decode(nonce_hex).map_err(|e| format!("Nonce hex decode failed: {}", e))?;

    let nonce_arr: [u8; 12] = nonce_bytes
        .try_into()
        .map_err(|_| "Nonce must be 12 bytes".to_string())?;

    let cipher = Aes256Gcm::new_from_slice(shared_secret)
        .map_err(|e| format!("AES key init failed: {}", e))?;

    cipher
        .decrypt((&nonce_arr).into(), ciphertext.as_slice())
        .map_err(|e| format!("AES-GCM decrypt failed: {}", e))
}
