//! Server-to-Server request signing and verification.
//!
//! Every outbound S2S HTTP request includes three headers:
//! `X-Magnolia-Timestamp` — Unix timestamp (seconds)
//! `X-Magnolia-Nonce` — random UUID, prevents replay within the same second
//! `X-Magnolia-Signature` — hex(ML-DSA-87 sign(sha256(body) || timestamp || nonce))
//!
//! The receiving server:
//! 1. Rejects if timestamp is more than 300 s away from now.
//! 2. Looks up the sender's ML-DSA verifying key by `X-Magnolia-Server`.
//! 3. Verifies the signature over the same message.
//!
//! The signed message is:
//! SHA-256(body) ++ timestamp_be_8bytes ++ nonce_utf8
//!
//! Using SHA-256(body) rather than the raw body keeps the signed message small
//! regardless of payload size and avoids copying large blobs into the signer.

use chrono::Utc;
use ml_dsa::{
    MlDsa87, SigningKey, VerifyingKey,
    signature::{Signer, Verifier},
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::models::S2SRequestMeta;

pub const HEADER_SERVER: &str = "X-Magnolia-Server";
pub const HEADER_TIMESTAMP: &str = "X-Magnolia-Timestamp";
pub const HEADER_NONCE: &str = "X-Magnolia-Nonce";
pub const HEADER_SIGNATURE: &str = "X-Magnolia-Signature";
/// Present (value "1") when the HTTP body is AES-256-GCM encrypted with the
/// ML-KEM shared secret. The encryption nonce is in HEADER_BODY_NONCE.
pub const HEADER_ENCRYPTED: &str = "X-Magnolia-Encrypted";
/// Hex-encoded 12-byte AES-GCM nonce for the encrypted body / response.
pub const HEADER_BODY_NONCE: &str = "X-Magnolia-Body-Nonce";

/// Maximum clock skew (seconds) before a request is rejected as stale.
pub const MAX_SKEW_SECS: i64 = 300;

// Signing

/// Build the message that is signed / verified.
fn signed_message(body: &[u8], timestamp: i64, nonce: &str) -> Vec<u8> {
    let body_hash = Sha256::digest(body);
    let ts_bytes = timestamp.to_be_bytes();
    let mut msg = Vec::with_capacity(32 + 8 + nonce.len());
    msg.extend_from_slice(&body_hash);
    msg.extend_from_slice(&ts_bytes);
    msg.extend_from_slice(nonce.as_bytes());
    msg
}

/// Sign a request body, returning `(timestamp, nonce, signature_hex)`.
pub fn sign_request(body: &[u8], signing_key: &SigningKey<MlDsa87>) -> (i64, String, String) {
    let timestamp = Utc::now().timestamp();
    let nonce = Uuid::new_v4().to_string();
    let msg = signed_message(body, timestamp, &nonce);
    let signature = signing_key.sign(&msg);
    let sig_hex = hex::encode(signature.encode().as_slice());
    (timestamp, nonce, sig_hex)
}

// Verification

#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("Missing required S2S header: {0}")]
    MissingHeader(&'static str),
    #[error("Invalid timestamp format")]
    BadTimestamp,
    #[error("Request timestamp too far from server time (skew > {MAX_SKEW_SECS}s)")]
    StaleTimestamp,
    #[error("Invalid signature encoding")]
    BadSignatureEncoding,
    #[error("Signature verification failed")]
    BadSignature,
}

/// Extract and validate S2S headers from an Axum `HeaderMap`.
pub fn extract_meta(headers: &axum::http::HeaderMap) -> Result<S2SRequestMeta, VerifyError> {
    let sender_address = headers
        .get(HEADER_SERVER)
        .and_then(|v| v.to_str().ok())
        .ok_or(VerifyError::MissingHeader(HEADER_SERVER))?
        .to_string();

    let timestamp: i64 = headers
        .get(HEADER_TIMESTAMP)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .ok_or(VerifyError::BadTimestamp)?;

    let nonce = headers
        .get(HEADER_NONCE)
        .and_then(|v| v.to_str().ok())
        .ok_or(VerifyError::MissingHeader(HEADER_NONCE))?
        .to_string();

    let sig_hex = headers
        .get(HEADER_SIGNATURE)
        .and_then(|v| v.to_str().ok())
        .ok_or(VerifyError::MissingHeader(HEADER_SIGNATURE))?;

    let signature = hex::decode(sig_hex).map_err(|_| VerifyError::BadSignatureEncoding)?;

    Ok(S2SRequestMeta {
        sender_address,
        timestamp,
        nonce,
        signature,
    })
}

/// Verify timestamp freshness and the ML-DSA signature over the request body.
pub fn verify_request(
    body: &[u8],
    meta: &S2SRequestMeta,
    verifying_key: &VerifyingKey<MlDsa87>,
) -> Result<(), VerifyError> {
    // Check clock skew.
    let now = Utc::now().timestamp();
    if (now - meta.timestamp).abs() > MAX_SKEW_SECS {
        return Err(VerifyError::StaleTimestamp);
    }

    // Rebuild the signed message.
    let msg = signed_message(body, meta.timestamp, &meta.nonce);

    // Decode and verify the ML-DSA signature.
    use ml_dsa::Signature;
    let mut sig_arr = ml_dsa::EncodedSignature::<MlDsa87>::default();
    if meta.signature.len() != sig_arr.len() {
        return Err(VerifyError::BadSignatureEncoding);
    }
    sig_arr.as_mut_slice().copy_from_slice(&meta.signature);
    let signature =
        Signature::<MlDsa87>::decode(&sig_arr).ok_or(VerifyError::BadSignatureEncoding)?;

    verifying_key
        .verify(&msg, &signature)
        .map_err(|_| VerifyError::BadSignature)
}

/// Decode an ML-DSA-87 verifying key from raw bytes.
pub fn decode_verifying_key(bytes: &[u8]) -> Result<VerifyingKey<MlDsa87>, String> {
    let mut arr = ml_dsa::EncodedVerifyingKey::<MlDsa87>::default();
    if bytes.len() != arr.len() {
        return Err(format!(
            "Verifying key has wrong length: {} (expected {})",
            bytes.len(),
            arr.len()
        ));
    }
    arr.as_mut_slice().copy_from_slice(bytes);
    Ok(VerifyingKey::decode(&arr))
}
