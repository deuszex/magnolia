//! Signed outbound HTTP client for S2S requests.
//!
//! All methods accept the sender's [`ServerIdentity`] and the target server's `address` (BASE_URL).
//! They build the request body as JSON, sign the bytes,
//! attach the S2S headers (like identity, signature, timestamp, nonce), and POST to the appropriate endpoint.
//!
//! Post-handshake functions also accept a `shared_secret` and encrypt the body
//! with AES-256-GCM before signing. The `X-Magnolia-Encrypted: 1` header tells
//! the receiver to decrypt before parsing. GET endpoints that return sensitive
//! data expect the server to respond with an encrypted body and decrypt it here.

use reqwest::Client;
use serde::Serialize;
use tracing::warn;

use super::{
    handshake::{decrypt_s2s_payload, encrypt_s2s_payload},
    identity::ServerIdentity,
    models::{
        ConnectionAccept, ConnectionReject, ConnectionRequest, DiscoveryPush,
        FederatedPostListResponse, S2SMessageEnvelope, S2SPostsRequest, UserIdentityResponse,
        UserListSync,
    },
    signing::{
        HEADER_BODY_NONCE, HEADER_ENCRYPTED, HEADER_NONCE, HEADER_SERVER, HEADER_SIGNATURE,
        HEADER_TIMESTAMP, sign_request,
    },
};

pub type S2SClient = Client;

pub fn build_client() -> S2SClient {
    Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("Failed to build S2S HTTP client")
}

/// Main send function
async fn post_signed<T: Serialize>(
    client: &S2SClient,
    identity: &ServerIdentity,
    our_address: &str,
    url: &str,
    body: &T,
) -> Result<reqwest::Response, String> {
    let body_bytes =
        serde_json::to_vec(body).map_err(|e| format!("Failed to serialize request body: {}", e))?;

    let signing_key = identity.signing_key();
    let (timestamp, nonce, sig_hex) = sign_request(&body_bytes, &signing_key);

    client
        .post(url)
        .header("Content-Type", "application/json")
        .header(HEADER_SERVER, our_address)
        .header(HEADER_TIMESTAMP, timestamp.to_string())
        .header(HEADER_NONCE, &nonce)
        .header(HEADER_SIGNATURE, &sig_hex)
        .body(body_bytes)
        .send()
        .await
        .map_err(|e| format!("S2S request to {} failed: {}", url, e))
}

/// Serialize, encrypt with AES-256-GCM, sign the ciphertext, and POST.
/// The receiver must have `X-Magnolia-Encrypted: 1` handling and the same
/// shared secret to decrypt before parsing the body.
async fn post_signed_encrypted<T: Serialize>(
    client: &S2SClient,
    identity: &ServerIdentity,
    our_address: &str,
    url: &str,
    body: &T,
    shared_secret: &[u8],
) -> Result<reqwest::Response, String> {
    let json_bytes =
        serde_json::to_vec(body).map_err(|e| format!("Failed to serialize request body: {}", e))?;

    let (ct_b64, nonce_hex) = encrypt_s2s_payload(shared_secret, &json_bytes)?;
    let ct_bytes = ct_b64.into_bytes();

    let signing_key = identity.signing_key();
    let (timestamp, nonce, sig_hex) = sign_request(&ct_bytes, &signing_key);

    client
        .post(url)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header(HEADER_SERVER, our_address)
        .header(HEADER_TIMESTAMP, timestamp.to_string())
        .header(HEADER_NONCE, &nonce)
        .header(HEADER_SIGNATURE, &sig_hex)
        .header(HEADER_ENCRYPTED, "1")
        .header(HEADER_BODY_NONCE, nonce_hex)
        .body(ct_bytes)
        .send()
        .await
        .map_err(|e| format!("S2S request to {} failed: {}", url, e))
}

/// Sign and send a GET request; returns the raw response.
async fn get_signed(
    client: &S2SClient,
    identity: &ServerIdentity,
    our_address: &str,
    url: &str,
) -> Result<reqwest::Response, String> {
    // For GET requests the signed body is the empty byte string.
    let empty: &[u8] = b"";
    let signing_key = identity.signing_key();
    let (timestamp, nonce, sig_hex) = sign_request(empty, &signing_key);

    client
        .get(url)
        .header(HEADER_SERVER, our_address)
        .header(HEADER_TIMESTAMP, timestamp.to_string())
        .header(HEADER_NONCE, &nonce)
        .header(HEADER_SIGNATURE, &sig_hex)
        .send()
        .await
        .map_err(|e| format!("S2S GET {} failed: {}", url, e))
}

/// Decrypt a response body if the server marked it with `X-Magnolia-Encrypted`.
async fn decrypt_response(
    resp: reqwest::Response,
    shared_secret: &[u8],
) -> Result<Vec<u8>, String> {
    let is_encrypted = resp
        .headers()
        .get(HEADER_ENCRYPTED)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "1")
        .unwrap_or(false);

    if !is_encrypted {
        return resp
            .bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| format!("Failed to read response body: {}", e));
    }

    let nonce_hex = resp
        .headers()
        .get(HEADER_BODY_NONCE)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| "Encrypted response missing X-Magnolia-Body-Nonce".to_string())?
        .to_string();

    let body_bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read encrypted response body: {}", e))?;

    let ct_b64 = std::str::from_utf8(&body_bytes)
        .map_err(|_| "Encrypted response body is not valid UTF-8".to_string())?;

    decrypt_s2s_payload(shared_secret, ct_b64, &nonce_hex)
}

// Connection handshake

/// POST a connection request to `peer_address/api/s2s/connect`.
pub async fn send_connection_request(
    client: &S2SClient,
    identity: &ServerIdentity,
    our_address: &str,
    peer_address: &str,
    ek_hex: String,
    is_shareable: bool,
    wants_discovery: bool,
    request_id: Option<String>,
) -> Result<(), String> {
    let url = format!("{}/api/s2s/connect", peer_address.trim_end_matches('/'));
    let payload = ConnectionRequest {
        address: our_address.to_string(),
        ml_dsa_public_key: identity.public_key_hex(),
        ml_kem_encap_key: ek_hex,
        is_shareable,
        wants_discovery,
        request_id,
    };
    let resp = post_signed(client, identity, our_address, &url, &payload).await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Connection request to {} rejected: {} — {}",
            peer_address, status, body
        ));
    }
    Ok(())
}

/// POST a connection acceptance to `peer_address/api/s2s/connect/accept`.
pub async fn send_connection_accept(
    client: &S2SClient,
    identity: &ServerIdentity,
    our_address: &str,
    peer_address: &str,
    ct_hex: String,
    is_shareable: bool,
    wants_discovery: bool,
    request_id: Option<String>,
) -> Result<(), String> {
    let url = format!(
        "{}/api/s2s/connect/accept",
        peer_address.trim_end_matches('/')
    );
    let payload = ConnectionAccept {
        address: our_address.to_string(),
        ml_dsa_public_key: identity.public_key_hex(),
        ml_kem_ciphertext: ct_hex,
        is_shareable,
        wants_discovery,
        request_id,
    };
    let resp = post_signed(client, identity, our_address, &url, &payload).await?;
    if !resp.status().is_success() {
        return Err(format!(
            "Accept delivery to {} failed: {}",
            peer_address,
            resp.status()
        ));
    }
    Ok(())
}

/// POST a connection rejection to `peer_address/api/s2s/connect/reject`.
pub async fn send_connection_reject(
    client: &S2SClient,
    identity: &ServerIdentity,
    our_address: &str,
    peer_address: &str,
    reason: Option<String>,
) -> Result<(), String> {
    let url = format!(
        "{}/api/s2s/connect/reject",
        peer_address.trim_end_matches('/')
    );
    let payload = ConnectionReject {
        address: our_address.to_string(),
        reason,
    };
    // Best-effort — don't fail our own operation if the peer is unreachable.
    if let Err(e) = post_signed(client, identity, our_address, &url, &payload).await {
        warn!(
            "Could not deliver rejection notice to {}: {}",
            peer_address, e
        );
    }
    Ok(())
}

/// POST a disconnect notice to `peer_address/api/s2s/disconnect`.
/// Best-effort — used when revoking a connection so the peer stops reconnecting.
pub async fn send_disconnect(
    client: &S2SClient,
    identity: &ServerIdentity,
    our_address: &str,
    peer_address: &str,
) -> Result<(), String> {
    let url = format!("{}/api/s2s/disconnect", peer_address.trim_end_matches('/'));
    let payload = serde_json::json!({ "address": our_address });
    if let Err(e) = post_signed(client, identity, our_address, &url, &payload).await {
        warn!(
            "Could not deliver disconnect notice to {}: {}",
            peer_address, e
        );
    }
    Ok(())
}

// User list sync

/// POST a user list update to `peer_address/api/s2s/users/sync`, encrypted.
pub async fn send_user_list_sync(
    client: &S2SClient,
    identity: &ServerIdentity,
    our_address: &str,
    peer_address: &str,
    sync: UserListSync,
    shared_secret: &[u8],
) -> Result<(), String> {
    let url = format!("{}/api/s2s/users/sync", peer_address.trim_end_matches('/'));
    let resp =
        post_signed_encrypted(client, identity, our_address, &url, &sync, shared_secret).await?;
    if !resp.status().is_success() {
        return Err(format!(
            "User list sync to {} failed: {}",
            peer_address,
            resp.status()
        ));
    }
    Ok(())
}

// Message delivery

/// POST an encrypted message envelope to `peer_address/api/s2s/message`.
/// The envelope body is additionally wrapped in transport-level encryption.
pub async fn send_message(
    client: &S2SClient,
    identity: &ServerIdentity,
    our_address: &str,
    peer_address: &str,
    envelope: S2SMessageEnvelope,
    shared_secret: &[u8],
) -> Result<(), String> {
    let url = format!("{}/api/s2s/message", peer_address.trim_end_matches('/'));
    let resp = post_signed_encrypted(
        client,
        identity,
        our_address,
        &url,
        &envelope,
        shared_secret,
    )
    .await?;
    if !resp.status().is_success() {
        return Err(format!(
            "Message delivery to {} failed: {}",
            peer_address,
            resp.status()
        ));
    }
    Ok(())
}

// Discovery relay

/// POST a discovery hint push to `peer_address/api/s2s/discovery`, encrypted.
pub async fn send_discovery(
    client: &S2SClient,
    identity: &ServerIdentity,
    our_address: &str,
    peer_address: &str,
    push: DiscoveryPush,
    shared_secret: &[u8],
) -> Result<(), String> {
    let url = format!("{}/api/s2s/discovery", peer_address.trim_end_matches('/'));
    // Best-effort — discovery is advisory, not critical.
    if let Err(e) =
        post_signed_encrypted(client, identity, our_address, &url, &push, shared_secret).await
    {
        warn!("Discovery push to {} failed: {}", peer_address, e);
    }
    Ok(())
}

// Call signaling

/// POST a call signal payload to `peer_address/api/s2s/call-signal`, encrypted.
/// Used to deliver call_incoming, call_accept, call_reject, sdp_offer, ice_candidate, etc.
pub async fn send_call_signal(
    client: &S2SClient,
    identity: &ServerIdentity,
    our_address: &str,
    peer_address: &str,
    payload: &serde_json::Value,
    shared_secret: &[u8],
) -> Result<(), String> {
    let url = format!("{}/api/s2s/call-signal", peer_address.trim_end_matches('/'));
    let resp =
        post_signed_encrypted(client, identity, our_address, &url, payload, shared_secret).await?;
    if !resp.status().is_success() {
        return Err(format!(
            "Call signal delivery to {} failed: {}",
            peer_address,
            resp.status()
        ));
    }
    Ok(())
}

// Post fetching

/// POST to `peer_address/api/s2s/posts` and parse the federated post list.
/// The request body is encrypted; the response is expected to be encrypted too.
/// Connection will only give user if the user has set federation visibility.
pub async fn fetch_posts(
    client: &S2SClient,
    identity: &ServerIdentity,
    our_address: &str,
    peer_address: &str,
    req: &S2SPostsRequest,
    shared_secret: &[u8],
) -> Result<FederatedPostListResponse, String> {
    let url = format!("{}/api/s2s/posts", peer_address.trim_end_matches('/'));
    let resp =
        post_signed_encrypted(client, identity, our_address, &url, req, shared_secret).await?;
    if !resp.status().is_success() {
        return Err(format!(
            "Post fetch from {} failed: {}",
            peer_address,
            resp.status()
        ));
    }
    let body = decrypt_response(resp, shared_secret).await?;
    serde_json::from_slice::<FederatedPostListResponse>(&body).map_err(|e| {
        format!(
            "Failed to parse post list response from {}: {}",
            peer_address, e
        )
    })
}

// Media fetch

/// GET `peer_address/api/s2s/media/{media_id}?remote_user_id={requesting_user_id}`
/// and return the raw file bytes.
///
/// `requesting_user_id` is the local user triggering the fetch — sent so the
/// peer can enforce its block/ban rules before streaming the file.
pub async fn fetch_media(
    client: &S2SClient,
    identity: &ServerIdentity,
    our_address: &str,
    peer_address: &str,
    media_id: &str,
    requesting_user_id: &str,
) -> Result<(Vec<u8>, String), String> {
    let url = format!(
        "{}/api/s2s/media/{}?remote_user_id={}",
        peer_address.trim_end_matches('/'),
        urlencoding::encode(media_id),
        urlencoding::encode(requesting_user_id),
    );
    let resp = get_signed(client, identity, our_address, &url).await?;

    if resp.status() == reqwest::StatusCode::FORBIDDEN {
        return Err(format!("Media {} on {} is blocked for this user", media_id, peer_address));
    }
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(format!("Media {} not found on {}", media_id, peer_address));
    }
    if !resp.status().is_success() {
        return Err(format!(
            "Media fetch {} from {} failed: {}",
            media_id, peer_address, resp.status()
        ));
    }

    // Extract Content-Type before consuming the response
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let body = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read media response body: {}", e))?
        .to_vec();

    Ok((body, content_type))
}

// User identity fetch

/// GET `peer_address/api/s2s/users/identity/{username}` and parse the response.
///
/// Returns `Ok(Some(identity))` if found, `Ok(None)` if the peer returns 404,
/// and `Err(reason)` for network errors or other non-success responses.
/// The response body is expected to be encrypted with the shared secret.
/// Connection will only give user if the user has set federation visibility.
pub async fn fetch_user_identity(
    client: &S2SClient,
    identity: &ServerIdentity,
    our_address: &str,
    peer_address: &str,
    username: &str,
    shared_secret: &[u8],
) -> Result<Option<UserIdentityResponse>, String> {
    let url = format!(
        "{}/api/s2s/users/identity/{}",
        peer_address.trim_end_matches('/'),
        urlencoding::encode(username),
    );
    let resp = get_signed(client, identity, our_address, &url).await?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(format!(
            "Identity fetch from {} for '{}' failed: {}",
            peer_address,
            username,
            resp.status()
        ));
    }
    let body = decrypt_response(resp, shared_secret).await?;
    serde_json::from_slice::<UserIdentityResponse>(&body)
        .map(Some)
        .map_err(|e| {
            format!(
                "Failed to parse identity response from {}: {}",
                peer_address, e
            )
        })
}
