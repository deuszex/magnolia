//! Axum handlers for federation endpoints.
//!
//! ## Endpoint groups
//!
//! ### Admin (require_admin middleware)
//! GET /api/admin/federation/settings
//! PUT /api/admin/federation/settings
//! GET /api/admin/federation/connections
//! POST /api/admin/federation/connections — initiate outgoing
//! GET /api/admin/federation/connections/:id
//! PUT /api/admin/federation/connections/:id — rename / notes
//! DELETE /api/admin/federation/connections/:id — revoke
//! POST /api/admin/federation/connections/:id/accept
//! POST /api/admin/federation/connections/:id/reject
//! GET /api/admin/federation/discovery
//! POST /api/admin/federation/discovery/:id/connect
//! DELETE /api/admin/federation/discovery/:id
//!
//! ### S2S inbound (no user auth — verified via ML-DSA signature)
//! POST /api/s2s/connect
//! POST /api/s2s/connect/accept
//! POST /api/s2s/connect/reject
//! POST /api/s2s/users/sync
//! POST /api/s2s/message
//! POST /api/s2s/discovery
//! GET /api/s2s/users/:user_id/ecdh-key
//!
//! ### User-facing (require_auth middleware)
//! GET /api/federation/users — search remote users
//! GET /api/users/federation-settings
//! PUT /api/users/federation-settings
//! POST /api/users/federation-bans
//! DELETE /api/users/federation-bans/:server_id/:remote_user_id

use axum::{
    Json,
    body::Body,
    extract::ws::WebSocketUpgrade,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sqlx::AnyPool;
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

use super::{
    client, handshake,
    identity::{ServerIdentity, load_shared_secret},
    models::{
        ConnectionAccept, ConnectionReject, ConnectionRequest, ConnectionStatus, DiscoveryPush,
        FederatedPostListResponse, FederationUserDto, S2SInnerMessage, S2SMessageEnvelope,
        S2SPostsRequest, ServerConnectionDto, UserIdentityResponse, UserListSync,
    },
    repo,
    signing::{
        HEADER_BODY_NONCE, HEADER_ENCRYPTED, decode_verifying_key, extract_meta, verify_request,
    },
};
use crate::events::{create_admin_event, record_federation_violation};
use crate::{
    config::Settings,
    middleware::AuthMiddleware,
    utils::encryption::{ContentEncryption, encrypt_text},
};

pub type AppState = (AnyPool, Arc<Settings>);

// Shared state helpers

fn pool(state: &AppState) -> &AnyPool {
    &state.0
}

fn settings(state: &AppState) -> &Arc<Settings> {
    &state.1
}

// Admin: federation settings

#[derive(Debug, Serialize, Deserialize)]
pub struct FederationSettingsDto {
    pub federation_enabled: bool,
    pub accept_incoming: bool,
    pub share_connections: bool,
    pub max_connections: i64,
    pub relay_depth: i64,
}

pub async fn admin_get_federation_settings(State(state): State<AppState>) -> impl IntoResponse {
    let row = sqlx::query(
        "SELECT federation_enabled, federation_accept_incoming,
 federation_share_connections, federation_max_connections,
 federation_relay_depth
 FROM site_config WHERE id = 1",
    )
    .fetch_one(pool(&state))
    .await;

    match row {
        Ok(r) => {
            use sqlx::Row;
            Json(FederationSettingsDto {
                federation_enabled: r.get::<i32, _>("federation_enabled") != 0,
                accept_incoming: r.get::<i32, _>("federation_accept_incoming") != 0,
                share_connections: r.get::<i32, _>("federation_share_connections") != 0,
                max_connections: r.get::<i64, _>("federation_max_connections"),
                relay_depth: r.get::<i64, _>("federation_relay_depth"),
            })
            .into_response()
        }
        Err(e) => {
            error!("Failed to load federation settings: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn admin_update_federation_settings(
    State(state): State<AppState>,
    Json(body): Json<FederationSettingsDto>,
) -> impl IntoResponse {
    let result = sqlx::query(
        "UPDATE site_config SET
 federation_enabled = $1,
 federation_accept_incoming = $2,
 federation_share_connections = $3,
 federation_max_connections = $4,
 federation_relay_depth = $5
 WHERE id = 1",
    )
    .bind(body.federation_enabled as i32)
    .bind(body.accept_incoming as i32)
    .bind(body.share_connections as i32)
    .bind(body.max_connections)
    .bind(body.relay_depth)
    .execute(pool(&state))
    .await;

    match result {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => {
            error!("Failed to update federation settings: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// Admin: connection management

pub async fn admin_list_connections(State(state): State<AppState>) -> impl IntoResponse {
    match repo::list_connections(pool(&state)).await {
        Ok(rows) => {
            let dtos: Vec<ServerConnectionDto> =
                rows.into_iter().map(ServerConnectionDto::from).collect();
            Json(dtos).into_response()
        }
        Err(e) => {
            error!("Failed to list connections: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn admin_get_connection(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match repo::get_connection_by_id(pool(&state), &id).await {
        Ok(Some(row)) => Json(ServerConnectionDto::from(row)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            error!("DB error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct InitiateConnectionRequest {
    pub address: String,
    pub we_share_connections: Option<bool>,
    pub display_name: Option<String>,
}

/// Admin initiates an outgoing connection request to a peer.
pub async fn admin_initiate_connection(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<Arc<ServerIdentity>>,
    axum::Extension(s2s_client): axum::Extension<client::S2SClient>,
    Json(body): Json<InitiateConnectionRequest>,
) -> impl IntoResponse {
    let peer_address = body.address.trim_end_matches('/').to_string();

    // Check for duplicate. Allow re-initiating if the prior attempt was rejected or revoked.
    match repo::get_connection_by_address(pool(&state), &peer_address).await {
        Ok(Some(existing)) => {
            if existing.status == "rejected" || existing.status == "revoked" {
                if let Err(e) = repo::delete_connection(pool(&state), &existing.id).await {
                    error!("DB error clearing old connection: {}", e);
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            } else {
                return (
                    StatusCode::CONFLICT,
                    "Connection to that address already exists",
                )
                    .into_response();
            }
        }
        Err(e) => {
            error!("DB error: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        Ok(None) => {}
    }

    // Generate ephemeral ML-KEM keypair.
    let (dk_bytes, ek_hex) = handshake::generate_handshake_keys();

    let we_share = body.we_share_connections.unwrap_or(false);

    // Persist as pending_out, storing our decapsulation key temporarily.
    let conn_id =
        match repo::insert_pending_out_connection(pool(&state), &peer_address, &dk_bytes, we_share)
            .await
        {
            Ok(id) => id,
            Err(e) => {
                error!("Failed to store pending connection: {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    // Persist optional admin-assigned display name.
    if let Some(ref name) = body.display_name {
        if let Err(e) =
            repo::update_connection_display_name(pool(&state), &conn_id, Some(name)).await
        {
            warn!("Failed to set display name: {}", e);
        }
    }

    let our_address = settings(&state).base_url.clone();
    let is_shareable = {
        let r = sqlx::query_scalar::<_, i32>(
            "SELECT federation_accept_incoming FROM site_config WHERE id = 1",
        )
        .fetch_one(pool(&state))
        .await
        .unwrap_or(0);
        r != 0
    };

    // Send the request asynchronously — don't block the admin response.
    let client_clone = s2s_client.clone();
    let id_clone = Arc::clone(&identity);
    let addr_clone = our_address.clone();
    let peer_clone = peer_address.clone();
    let conn_id_clone = conn_id.clone();

    tokio::spawn(async move {
        if let Err(e) = client::send_connection_request(
            &client_clone,
            &id_clone,
            &addr_clone,
            &peer_clone,
            ek_hex,
            is_shareable,
            true,
            Some(conn_id_clone),
        )
        .await
        {
            warn!("Connection request to {} failed: {}", peer_clone, e);
        }
    });

    Json(serde_json::json!({ "id": conn_id, "status": "pending_out" })).into_response()
}

#[derive(Debug, Deserialize)]
pub struct UpdateConnectionRequest {
    pub display_name: Option<String>,
    pub notes: Option<String>,
}

pub async fn admin_update_connection(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateConnectionRequest>,
) -> impl IntoResponse {
    if let Some(name) = &body.display_name {
        if let Err(e) = repo::update_connection_display_name(pool(&state), &id, Some(name)).await {
            error!("DB error: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    if let Err(e) = repo::update_connection_notes(pool(&state), &id, body.notes.as_deref()).await {
        error!("DB error: {}", e);
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    StatusCode::OK.into_response()
}

/// Admin accepts a pending_in connection request.
pub async fn admin_accept_connection(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<Arc<ServerIdentity>>,
    axum::Extension(s2s_client): axum::Extension<client::S2SClient>,
    axum::Extension(registry): axum::Extension<crate::handlers::ws::ConnectionRegistry>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let enc = settings(&state)
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| ContentEncryption::from_hex_key(key).ok());
    let conn = match repo::get_connection_by_id(pool(&state), &id).await {
        Ok(Some(c)) if c.status == ConnectionStatus::PendingIn.as_str() => c,
        Ok(Some(_)) => {
            return (
                StatusCode::BAD_REQUEST,
                "Connection is not in pending_in state",
            )
                .into_response();
        }
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            error!("DB error: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let ek_bytes = match &conn.their_ml_kem_encap_key {
        Some(b) => b,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                "No encapsulation key stored for this connection",
            )
                .into_response();
        }
    };

    let ek_hex = hex::encode(ek_bytes);
    let (ct_hex, ss_bytes) = match handshake::encapsulate(&ek_hex) {
        Ok(v) => v,
        Err(e) => {
            error!("ML-KEM encapsulation failed: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Encrypt the shared secret before storing.
    let stored_secret = match enc.as_ref() {
        Some(cipher) => match super::identity::encrypt_shared_secret(cipher, &ss_bytes) {
            Ok(b) => b,
            Err(e) => {
                error!("Failed to encrypt shared secret: {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        },
        None => super::identity::store_shared_secret_raw(&ss_bytes),
    };

    if let Err(e) = repo::activate_incoming_connection(pool(&state), &id, &stored_secret).await {
        error!("Failed to activate connection: {}", e);
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    // Notify the initiating server asynchronously.
    let our_address = settings(&state).base_url.clone();
    let peer_address = conn.address.clone();
    let is_shareable = sqlx::query_scalar::<_, i32>(
        "SELECT federation_accept_incoming FROM site_config WHERE id = 1",
    )
    .fetch_one(pool(&state))
    .await
    .unwrap_or(0)
        != 0;

    let client_clone = s2s_client.clone();
    let id_clone = Arc::clone(&identity);
    let peer_clone = peer_address.clone();
    let peer_request_id_clone = conn.peer_request_id.clone();
    tokio::spawn(async move {
        if let Err(e) = client::send_connection_accept(
            &client_clone,
            &id_clone,
            &our_address,
            &peer_clone,
            ct_hex,
            is_shareable,
            true,
            peer_request_id_clone,
        )
        .await
        {
            warn!("Failed to send accept to {}: {}", peer_clone, e);
        }
    });

    // Push our current user list to the new peer.
    // Small delay to let the accept be received and the connection activated on the peer side
    // before the sync arrives, the peer rejects syncs from non-active connections.
    let pool_clone = pool(&state).clone();
    let settings_clone = Arc::clone(settings(&state));
    let identity_clone = Arc::clone(&identity);
    let s2s_clone = s2s_client.clone();
    let peer_for_sync = peer_address.clone();
    let id_for_sync = id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        super::sync::push_user_list_to_peer(
            pool_clone,
            settings_clone,
            identity_clone,
            s2s_clone,
            peer_for_sync,
            id_for_sync,
        )
        .await;
    });

    // Push discovery hints to the new peer if they want them.
    let peer_wants_discovery = conn.peer_wants_discovery != 0;
    let pool_clone = pool(&state).clone();
    let settings_clone = Arc::clone(settings(&state));
    tokio::spawn(super::relay::push_discovery_on_connect(
        pool_clone,
        settings_clone,
        Arc::clone(&identity),
        s2s_client.clone(),
        peer_address.clone(),
        peer_wants_discovery,
        id.clone(),
    ));

    // Fetch and cache the peer's shareable posts.
    let pool_clone = pool(&state).clone();
    let settings_clone = Arc::clone(settings(&state));
    tokio::spawn(super::posts_sync::fetch_and_store_posts_from_peer(
        pool_clone,
        settings_clone,
        Arc::clone(&identity),
        s2s_client,
        peer_address.clone(),
        id.clone(),
    ));

    let peer_addr = peer_address.clone();
    // Open a persistent WS connection to this peer.
    if let Some(registry) = super::hub::global_registry()
        && let Some(status) = super::hub::global_status()
    {
        super::hub::spawn_outbound_for_peer(
            pool(&state).clone(),
            Arc::clone(settings(&state)),
            Arc::clone(&identity),
            registry.clone(),
            status.clone(),
            id,
            peer_address,
        );
    }

    let pool_clone = pool(&state).clone();
    let registry_clone = registry.clone();
    tokio::spawn(async move {
        create_admin_event(
            &pool_clone,
            Some(&registry_clone),
            "federation",
            "connection_accepted",
            "info",
            "Federation connection established",
            &format!("Connection with {} is now active.", peer_addr),
            None,
        )
        .await;
    });

    Json(serde_json::json!({ "status": "active" })).into_response()
}

#[derive(Debug, Deserialize)]
pub struct RejectConnectionBody {
    pub reason: Option<String>,
}

pub async fn admin_reject_connection(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<Arc<ServerIdentity>>,
    axum::Extension(s2s_client): axum::Extension<client::S2SClient>,
    Path(id): Path<String>,
    Json(body): Json<RejectConnectionBody>,
) -> impl IntoResponse {
    let conn = match repo::get_connection_by_id(pool(&state), &id).await {
        Ok(Some(c)) => c,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            error!("DB error: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if let Err(e) =
        repo::set_connection_status(pool(&state), &id, ConnectionStatus::Rejected.as_str()).await
    {
        error!("DB error: {}", e);
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let our_address = settings(&state).base_url.clone();
    let peer_address = conn.address.clone();
    let client_clone = s2s_client.clone();
    let id_clone = Arc::clone(&identity);
    let reason = body.reason.clone();
    tokio::spawn(async move {
        let _ = client::send_connection_reject(
            &client_clone,
            &id_clone,
            &our_address,
            &peer_address,
            reason,
        )
        .await;
    });

    StatusCode::OK.into_response()
}

pub async fn admin_revoke_connection(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<Arc<ServerIdentity>>,
    axum::Extension(s2s_client): axum::Extension<client::S2SClient>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // For connections that were never active (pending_out, rejected, revoked) just delete the row.
    // For active / pending_in connections mark as revoked to preserve audit history.
    let conn = match repo::get_connection_by_id(pool(&state), &id).await {
        Ok(Some(c)) => c,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            error!("DB error: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let was_active = conn.status == ConnectionStatus::Active.as_str();
    let peer_address = conn.address.clone();

    let result = if matches!(conn.status.as_str(), "pending_out" | "rejected" | "revoked") {
        repo::delete_connection(pool(&state), &id).await
    } else {
        repo::set_connection_status(pool(&state), &id, ConnectionStatus::Revoked.as_str()).await
    };

    match result {
        Ok(_) => {
            // Notify the peer so they stop reconnecting.
            if was_active {
                let our_address = settings(&state).base_url.clone();
                let id_clone = Arc::clone(&identity);
                let client_clone = s2s_client.clone();
                tokio::spawn(async move {
                    let _ = client::send_disconnect(
                        &client_clone,
                        &id_clone,
                        &our_address,
                        &peer_address,
                    )
                    .await;
                });
            }
            StatusCode::OK.into_response()
        }
        Err(e) => {
            error!("DB error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// Admin: discovery hints

pub async fn admin_list_discovery(State(state): State<AppState>) -> impl IntoResponse {
    match repo::list_discovery_hints(pool(&state)).await {
        Ok(rows) => {
            let dtos: Vec<_> = rows
                .into_iter()
                .map(super::models::DiscoveryHintDto::from)
                .collect();
            Json(dtos).into_response()
        }
        Err(e) => {
            error!("DB error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn admin_dismiss_discovery(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match repo::set_discovery_hint_status(pool(&state), &id, "dismissed").await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => {
            error!("DB error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// S2S inbound: connection handshake

/// Inbound connection request from Side A.
pub async fn s2s_receive_connect(
    State(state): State<AppState>,
    axum::Extension(registry): axum::Extension<crate::handlers::ws::ConnectionRegistry>,
    _headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Check that federation accepts incoming.
    let accepts: i32 =
        sqlx::query_scalar("SELECT federation_accept_incoming FROM site_config WHERE id = 1")
            .fetch_one(pool(&state))
            .await
            .unwrap_or(0);

    if accepts == 0 {
        return (
            StatusCode::FORBIDDEN,
            "This server does not accept incoming federation requests",
        )
            .into_response();
    }

    // Parse payload first so we can look up the sender's key if it's a known peer.
    let req: ConnectionRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    // For new connections we can't verify the signature yet (we don't have their key).
    // We store it as pending_in for admin review. The ML-DSA key they sent is
    // stored and will be used to verify all subsequent requests.
    // (Future: require a pre-shared invite token as an anti-spam measure.)

    // Check for duplicate. Allow re-handshake if the prior record was rejected or revoked.
    let address = req.address.trim_end_matches('/').to_string();
    match repo::get_connection_by_address(pool(&state), &address).await {
        Ok(Some(existing)) => {
            if existing.status == "rejected" || existing.status == "revoked" {
                if let Err(e) = repo::delete_connection(pool(&state), &existing.id).await {
                    error!("DB error clearing old connection on re-handshake: {}", e);
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            } else {
                return (StatusCode::CONFLICT, "Already connected or pending").into_response();
            }
        }
        Err(e) => {
            error!("DB error checking for duplicate connection: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        Ok(None) => {}
    }

    let dsa_key_bytes = match hex::decode(&req.ml_dsa_public_key) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid ML-DSA key").into_response(),
    };
    let kem_key_bytes = match hex::decode(&req.ml_kem_encap_key) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid ML-KEM key").into_response(),
    };

    match repo::insert_pending_in_connection(
        pool(&state),
        &address,
        &dsa_key_bytes,
        &kem_key_bytes,
        req.is_shareable,
        req.wants_discovery,
        req.request_id.as_deref(),
    )
    .await
    {
        Ok(_) => {
            info!("Received federation connection request from {}", address);
            let pool_clone = pool(&state).clone();
            let addr_clone = address.clone();
            let registry_clone = registry.clone();
            tokio::spawn(async move {
                create_admin_event(
                    &pool_clone,
                    Some(&registry_clone),
                    "federation",
                    "connection_request",
                    "warning",
                    "Incoming federation request",
                    &format!(
                        "{} wants to federate with this server. Review in Admin → Federation.",
                        addr_clone
                    ),
                    None,
                )
                .await;
            });
            StatusCode::OK.into_response()
        }
        Err(e) => {
            error!("Failed to store pending connection: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Inbound accept from Side B (other server), Side A (this server) decapsulates to get the shared secret.
pub async fn s2s_receive_accept(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<Arc<ServerIdentity>>,
    axum::Extension(s2s_client): axum::Extension<client::S2SClient>,
    axum::Extension(registry): axum::Extension<crate::handlers::ws::ConnectionRegistry>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let enc = settings(&state)
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| ContentEncryption::from_hex_key(key).ok());
    let accept: ConnectionAccept = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let peer_address = accept.address.trim_end_matches('/').to_string();

    // Find our pending_out record.
    // Prefer lookup by request_id (our own connection ID echoed back by Side B), this
    // is immune to base_url mismatches where the admin entered a different address from
    // what Side B reports as its base_url (e.g. 127.0.0.1 vs localhost).
    let conn_by_id = if let Some(ref rid) = accept.request_id {
        match repo::get_connection_by_id(pool(&state), rid).await {
            Ok(Some(c)) if c.status == ConnectionStatus::PendingOut.as_str() => Some(c),
            Ok(Some(c)) => {
                warn!(
                    "s2s_receive_accept: request_id lookup found record with status '{}', expected pending_out",
                    c.status
                );
                None
            }
            Ok(None) => None,
            Err(e) => {
                error!("DB error in s2s_receive_accept (id lookup): {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    } else {
        None
    };

    let conn = if let Some(c) = conn_by_id {
        // If the stored address differs from Side B's actual base_url, update it now so
        // future lookups (disconnect, verify, message routing) find the right record.
        if c.address != peer_address {
            info!(
                "s2s_receive_accept: updating stored address for {} from '{}' to '{}'",
                c.id, c.address, peer_address
            );
            let _ = repo::update_connection_address(pool(&state), &c.id, &peer_address).await;
        }
        c
    } else {
        // Fall back to address lookup for older peers that don't send request_id.
        match repo::get_connection_by_address(pool(&state), &peer_address).await {
            Ok(Some(c)) if c.status == ConnectionStatus::PendingOut.as_str() => c,
            Ok(Some(c)) => {
                warn!(
                    "s2s_receive_accept: got accept from {} but our record has status '{}', expected pending_out",
                    peer_address, c.status
                );
                return (
                    StatusCode::BAD_REQUEST,
                    "No matching pending_out connection",
                )
                    .into_response();
            }
            Ok(None) => {
                warn!(
                    "s2s_receive_accept: got accept from {} but no connection record found",
                    peer_address
                );
                return (
                    StatusCode::BAD_REQUEST,
                    "No matching pending_out connection",
                )
                    .into_response();
            }
            Err(e) => {
                error!("DB error in s2s_receive_accept: {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    };

    // Verify ML-DSA signature now that we have their key.
    let dsa_bytes = match hex::decode(&accept.ml_dsa_public_key) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid ML-DSA key").into_response(),
    };
    let verifying_key = match decode_verifying_key(&dsa_bytes) {
        Ok(k) => k,
        Err(e) => {
            warn!(
                "s2s_receive_accept: cannot decode ML-DSA key from {}: {}",
                peer_address, e
            );
            return (StatusCode::BAD_REQUEST, "Cannot decode ML-DSA key").into_response();
        }
    };
    let meta = match extract_meta(&headers) {
        Ok(m) => m,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    if verify_request(&body, &meta, &verifying_key).is_err() {
        return (StatusCode::UNAUTHORIZED, "Signature verification failed").into_response();
    }

    // Retrieve our stored decapsulation key.
    let dk_bytes = match &conn.their_ml_kem_encap_key {
        Some(b) => b.clone(),
        None => return (StatusCode::INTERNAL_SERVER_ERROR, "Missing DK").into_response(),
    };

    let ss_bytes = match handshake::decapsulate(&dk_bytes, &accept.ml_kem_ciphertext) {
        Ok(s) => s,
        Err(e) => {
            error!("Decapsulation failed: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let stored_secret = match enc.as_ref() {
        Some(cipher) => match super::identity::encrypt_shared_secret(cipher, &ss_bytes) {
            Ok(b) => b,
            Err(e) => {
                error!("Encrypt shared secret: {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        },
        None => super::identity::store_shared_secret_raw(&ss_bytes),
    };

    match repo::activate_connection(
        pool(&state),
        &conn.id,
        &dsa_bytes,
        &stored_secret,
        accept.is_shareable,
        accept.wants_discovery,
    )
    .await
    {
        Ok(_) => {
            info!("Connection with {} is now active", peer_address);

            // Push our user list to the peer that just accepted us.
            let pool_clone = pool(&state).clone();
            let settings_clone = Arc::clone(settings(&state));
            let conn_id_for_sync = conn.id.clone();
            tokio::spawn(super::sync::push_user_list_to_peer(
                pool_clone,
                settings_clone,
                Arc::clone(&identity),
                s2s_client.clone(),
                peer_address.clone(),
                conn_id_for_sync,
            ));

            // Push discovery hints if the peer wants them.
            let pool_clone = pool(&state).clone();
            let settings_clone = Arc::clone(settings(&state));
            let conn_id_for_disc = conn.id.clone();
            tokio::spawn(super::relay::push_discovery_on_connect(
                pool_clone,
                settings_clone,
                Arc::clone(&identity),
                s2s_client.clone(),
                peer_address.clone(),
                accept.wants_discovery,
                conn_id_for_disc,
            ));

            // Fetch and cache the peer's shareable posts.
            let pool_clone = pool(&state).clone();
            let settings_clone = Arc::clone(settings(&state));
            let conn_id = conn.id.clone();
            tokio::spawn(super::posts_sync::fetch_and_store_posts_from_peer(
                pool_clone,
                settings_clone,
                Arc::clone(&identity),
                s2s_client,
                peer_address.clone(),
                conn_id.clone(),
            ));

            // Open a persistent WS connection to this peer.
            if let Some(hub_registry) = super::hub::global_registry()
                && let Some(status) = super::hub::global_status()
            {
                super::hub::spawn_outbound_for_peer(
                    pool(&state).clone(),
                    Arc::clone(settings(&state)),
                    Arc::clone(&identity),
                    hub_registry.clone(),
                    status.clone(),
                    conn_id,
                    peer_address.clone(),
                );
            }

            // Notify admins that this server's outgoing connection was accepted.
            let pool_clone = pool(&state).clone();
            let peer_addr_clone = peer_address.clone();
            let registry_clone = registry.clone();
            tokio::spawn(async move {
                create_admin_event(
                    &pool_clone,
                    Some(&registry_clone),
                    "federation",
                    "connection_established",
                    "info",
                    "Federation connection established",
                    &format!("Connection with {} is now active.", peer_addr_clone),
                    None,
                )
                .await;
            });

            StatusCode::OK.into_response()
        }
        Err(e) => {
            error!("Failed to activate connection: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Inbound disconnect notice about the other peer intentionally revoking this connection.
pub async fn s2s_receive_disconnect(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let conn = match get_verified_connection(
        pool(&state),
        &match extract_meta(&headers) {
            Ok(m) => m,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        },
        &body,
    )
    .await
    {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let _ = repo::set_connection_status(pool(&state), &conn.id, ConnectionStatus::Revoked.as_str())
        .await;
    info!(
        "Received disconnect notice from {}; connection marked revoked",
        conn.address
    );
    StatusCode::OK.into_response()
}

/// Inbound rejection from Side B.
pub async fn s2s_receive_reject(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let reject: ConnectionReject = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let peer_address = reject.address.trim_end_matches('/').to_string();

    if let Ok(Some(conn)) = repo::get_connection_by_address(pool(&state), &peer_address).await {
        let _ = repo::set_connection_status(
            pool(&state),
            &conn.id,
            ConnectionStatus::Rejected.as_str(),
        )
        .await;
    }

    StatusCode::OK.into_response()
}

// S2S inbound: user list sync

pub async fn s2s_receive_user_sync(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let meta = match extract_meta(&headers) {
        Ok(m) => m,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let conn = match get_verified_connection(pool(&state), &meta, &body).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let enc = settings(&state)
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| ContentEncryption::from_hex_key(key).ok());

    let plaintext = match decrypt_body_if_needed(&headers, &body, &conn, enc.as_ref()) {
        Ok(b) => b,
        Err(resp) => return resp,
    };

    let sync: UserListSync = match serde_json::from_slice(&plaintext) {
        Ok(s) => s,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    // Replace the stored user list for this server.
    if let Err(e) = repo::delete_all_federation_users_for_server(pool(&state), &conn.id).await {
        error!("Failed to clear federation users: {}", e);
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    for user in &sync.users {
        if let Err(e) = repo::upsert_federation_user(
            pool(&state),
            &conn.id,
            &user.remote_user_id,
            &user.username,
            user.display_name.as_deref(),
            user.avatar_url.as_deref(),
            user.ecdh_public_key.as_deref(),
        )
        .await
        {
            error!("Failed to upsert federation user: {}", e);
        }
    }

    let _ = repo::update_last_seen(pool(&state), &conn.id).await;
    StatusCode::OK.into_response()
}

// S2S inbound: call signal

/// POST /api/s2s/call-signal — receive a real-time call signal from a peer server.
/// The body is encrypted with the shared secret and signed.
pub async fn s2s_receive_call_signal(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let meta = match extract_meta(&headers) {
        Ok(m) => m,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let conn = match get_verified_connection(pool(&state), &meta, &body).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let enc = settings(&state)
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| ContentEncryption::from_hex_key(key).ok());

    let plaintext = match decrypt_body_if_needed(&headers, &body, &conn, enc.as_ref()) {
        Ok(b) => b,
        Err(resp) => return resp,
    };

    let payload: serde_json::Value = match serde_json::from_slice(&plaintext) {
        Ok(v) => v,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let signal_type = payload
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    info!(
        "s2s_receive_call_signal: {} from {}",
        signal_type, conn.address
    );
    info!("{payload}");
    super::hub::handle_inbound_call_signal(pool(&state), &conn.id, payload).await;

    StatusCode::OK.into_response()
}

// S2S inbound: message delivery

/// Create local media stubs for federated message attachments and link them to the message.
/// Runs as a separate spawned task to keep the parent handler's future small.
async fn create_message_media_stubs(
    pool: AnyPool,
    msg_repo: magnolia_common::repositories::MessageRepository,
    attachments: Vec<super::models::FederatedMediaRef>,
    peer_address: String,
    message_id: String,
) -> Vec<String> {
    use magnolia_common::repositories::MediaRepository;
    let media_repo = MediaRepository::new(pool);
    let now_str = chrono::Utc::now().to_rfc3339();
    let mut local_ids = Vec::new();
    for att in &attachments {
        let local_media_id = match media_repo
            .find_by_origin(&peer_address, &att.media_id)
            .await
        {
            Ok(Some(existing)) => existing.media_id,
            _ => {
                let new_id = Uuid::new_v4().to_string();
                if let Err(e) = media_repo
                    .create_federated_stub(
                        &new_id,
                        "__fed__",
                        &att.media_type,
                        &att.filename,
                        &att.mime_type,
                        att.file_size,
                        att.width,
                        att.height,
                        &peer_address,
                        &att.media_id,
                        &now_str,
                    )
                    .await
                {
                    warn!("create_message_media_stubs: stub failed: {}", e);
                    continue;
                }
                new_id
            }
        };
        let att_row_id = Uuid::new_v4().to_string();
        if let Err(e) = msg_repo
            .create_attachment(&att_row_id, &message_id, &local_media_id)
            .await
        {
            warn!("create_message_media_stubs: link failed: {}", e);
        } else {
            local_ids.push(local_media_id);
        }
    }
    local_ids
}

pub async fn s2s_receive_message(
    State(state): State<AppState>,
    axum::Extension(registry): axum::Extension<crate::handlers::ws::ConnectionRegistry>,
    axum::Extension(s2s_client): axum::Extension<client::S2SClient>,
    axum::Extension(server_identity): axum::Extension<Arc<ServerIdentity>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let meta = match extract_meta(&headers) {
        Ok(m) => m,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let conn = match get_verified_connection(pool(&state), &meta, &body).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    // Decrypt transport-level encryption (outer AES-256-GCM layer from client.rs).
    let enc = settings(&state)
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| ContentEncryption::from_hex_key(key).ok());

    let plaintext = match decrypt_body_if_needed(&headers, &body, &conn, enc.as_ref()) {
        Ok(b) => b,
        Err(resp) => return resp,
    };

    let envelope: S2SMessageEnvelope = match serde_json::from_slice(&plaintext) {
        Ok(e) => e,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    // Decrypt the inner per-message AES-256-GCM layer (E2E DM payload).
    let raw_secret = match conn.shared_secret.as_deref() {
        Some(s) => s.to_vec(),
        None => {
            error!("s2s_receive_message: no shared secret for conn {}", conn.id);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let shared_secret = match load_shared_secret(&raw_secret, enc.as_ref()) {
        Ok(s) => s,
        Err(e) => {
            error!("s2s_receive_message: decrypt shared secret failed: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let inner_bytes = match handshake::decrypt_s2s_payload(
        &shared_secret,
        &envelope.encrypted_payload,
        &envelope.nonce,
    ) {
        Ok(b) => b,
        Err(e) => {
            warn!(
                "s2s_receive_message: decrypt payload from {} failed: {}",
                conn.address, e
            );
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    let inner: S2SInnerMessage = match serde_json::from_slice(&inner_bytes) {
        Ok(i) => i,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    // Extract username from "username@server" format.
    let sender_username = match envelope.sender_qualified_id.split('@').next() {
        Some(uid) if !uid.is_empty() => uid.to_string(),
        _ => {
            warn!(
                "s2s_receive_message: invalid sender_qualified_id: {}",
                envelope.sender_qualified_id
            );
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    let recipient_id = inner.recipient_user_id.clone();

    // Sender identity verification
    // Look up the sender in the federation_users table by username.
    let known_sender =
        match repo::get_federation_user_by_username(pool(&state), &conn.id, &sender_username).await
        {
            Ok(u) => u,
            Err(e) => {
                error!("s2s_receive_message: db error looking up sender: {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    // If sender unknown, try a one-shot identity fetch from the remote server.
    let remote_user_id = if let Some(ref u) = known_sender {
        u.remote_user_id.clone()
    } else {
        info!(
            "s2s_receive_message: unknown sender '{}' from {}, attempting on-demand identity fetch",
            sender_username, conn.address
        );
        let our_address = settings(&state).base_url.trim_end_matches('/');
        let fetched = client::fetch_user_identity(
            &s2s_client,
            &server_identity,
            our_address,
            &conn.address,
            &sender_username,
            &shared_secret,
        )
        .await;

        match fetched {
            Ok(Some(identity)) => {
                let _ = repo::upsert_federation_user(
                    pool(&state),
                    &conn.id,
                    &identity.remote_user_id,
                    &identity.username,
                    identity.display_name.as_deref(),
                    identity.avatar_url.as_deref(),
                    identity.ecdh_public_key.as_deref(),
                )
                .await;
                info!(
                    "s2s_receive_message: resolved sender '{}' via on-demand fetch",
                    sender_username
                );
                identity.remote_user_id
            }
            Ok(None) | Err(_) => {
                // Fetch failed or denied, reject the message and notify admins.
                // If a federated user is trying to connect a local user without revealing themselves
                // through their own federation settings, we will qualify it as "harassment".
                // User accounts can be named and bio-d anyhow, but if you can't even notify
                // federated admin about the actor account, we flag it as malicious.
                warn!(
                    "s2s_receive_message: rejecting message from unverifiable sender '{}' at {}",
                    sender_username, conn.address
                );
                let violation_count = record_federation_violation(pool(&state), &conn.id).await;
                let priority = if violation_count >= 5 {
                    "critical"
                } else {
                    "warning"
                };
                create_admin_event(
                    pool(&state),
                    Some(&registry),
                    "federation",
                    "federation:unknown_sender",
                    priority,
                    &format!("Unknown sender from {}", conn.address),
                    &format!(
                        "Server '{}' sent a message from unverifiable sender '{}'. \
 Message rejected. Violation count: {}{}",
                        conn.address,
                        sender_username,
                        violation_count,
                        if violation_count >= 5 {
                            " — this server may be a bad actor."
                        } else {
                            "."
                        }
                    ),
                    Some(serde_json::json!({
                    "server_connection_id": conn.id,
                    "server_address": conn.address,
                    "sender_username": sender_username,
                    "violation_count": violation_count,
                    })),
                )
                .await;
                let _ = repo::update_last_seen(pool(&state), &conn.id).await;
                return StatusCode::FORBIDDEN.into_response();
            }
        }
    };

    // Silently drop messages from banned senders.
    match repo::is_external_user_banned(pool(&state), &recipient_id, &conn.id, &remote_user_id)
        .await
    {
        Ok(true) => {
            info!(
                "s2s_receive_message: dropping message from banned {}@{}",
                remote_user_id, conn.address
            );
            let _ = repo::update_last_seen(pool(&state), &conn.id).await;
            return StatusCode::OK.into_response();
        }
        Ok(false) => {}
        Err(e) => {
            error!("s2s_receive_message: ban check failed: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    // Find or create the local conversation for this message.
    // Group messages: map to a single local group via remote_group_id.
    // Direct messages: map to a per-sender federated DM (existing behaviour).
    let conversation_id = if envelope.conversation_type == "group" {
        // Look for a local group conversation that was already created for this
        // remote group + recipient pair.
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT c.conversation_id FROM conversations c
             JOIN conversation_members cm ON cm.conversation_id = c.conversation_id
             WHERE c.remote_group_id = $1 AND cm.user_id = $2
             LIMIT 1",
        )
        .bind(&envelope.conversation_id)
        .bind(&recipient_id)
        .fetch_optional(pool(&state))
        .await
        .unwrap_or(None);

        if let Some(id) = existing {
            id
        } else {
            // First message from this remote group, each server should create a local group conversation.
            let new_id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            let group_name = envelope.group_name.as_deref().unwrap_or("Federated Group");

            if let Err(e) = sqlx::query(
                "INSERT INTO conversations
                 (conversation_id, conversation_type, name, created_by, remote_group_id, created_at, updated_at)
                 VALUES ($1, 'group', $2, $3, $4, $5, $5)",
            )
            .bind(&new_id)
            .bind(group_name)
            .bind(&recipient_id)
            .bind(&envelope.conversation_id)
            .bind(&now)
            .execute(pool(&state))
            .await
            {
                error!("s2s_receive_message: create group conv failed: {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }

            // Add the local recipient as a member.
            if let Err(e) = sqlx::query(
                "INSERT INTO conversation_members (id, conversation_id, user_id, role, joined_at)
                 VALUES ($1, $2, $3, 'member', $4)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&new_id)
            .bind(&recipient_id)
            .bind(&now)
            .execute(pool(&state))
            .await
            {
                error!("s2s_receive_message: add group member failed: {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }

            // Register the sending server as a federated participant so future
            // messages from the same group arrive in the same conversation.
            let _ = repo::add_federated_member(
                pool(&state),
                &new_id,
                &conn.id,
                &remote_user_id,
                &envelope.sender_qualified_id,
            )
            .await;

            new_id
        }
    } else {
        // Direct message path, should be mirrored on the other server, find or create a per-sender federated DM.
        match repo::find_federated_dm(pool(&state), &recipient_id, &conn.id, &remote_user_id).await
        {
            Ok(Some(id)) => id,
            Ok(None) => {
                match repo::create_federated_dm(
                    pool(&state),
                    &recipient_id,
                    &conn.id,
                    &remote_user_id,
                    &envelope.sender_qualified_id,
                )
                .await
                {
                    Ok(id) => id,
                    Err(e) => {
                        error!("s2s_receive_message: create federated DM failed: {}", e);
                        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                    }
                }
            }
            Err(e) => {
                error!("s2s_receive_message: find federated DM failed: {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    };

    // Use the sender's queue_id as a stable message_id for idempotency:
    // if the sender retries after a transient failure, the INSERT OR IGNORE means
    // we won't create a duplicate message on the receiving side.
    let message_id = envelope.queue_id.clone();
    let (stored_content, content_nonce) =
        match encrypt_text(settings(&state), pool(&state), &inner.encrypted_content).await {
            Ok(v) => v,
            Err(e) => {
                error!("s2s_receive_message: encrypt content failed: {:?}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
    let msg = magnolia_common::models::Message {
        message_id: message_id.clone(),
        conversation_id: conversation_id.clone(),
        sender_id: "__fed__".to_string(),
        remote_sender_qualified_id: Some(envelope.sender_qualified_id.clone()),
        proxy_sender_id: None,
        encrypted_content: stored_content.clone(),
        created_at: inner.sent_at.clone(),
        federated_status: None, // inbound, no outbound status
        content_nonce: content_nonce.clone(),
    };

    let msg_repo = magnolia_common::repositories::MessageRepository::new(pool(&state).clone());
    // INSERT OR IGNORE: silently discard duplicate deliveries from retries.
    match sqlx::query(
        r#"INSERT OR IGNORE INTO messages
           (message_id, conversation_id, sender_id, remote_sender_qualified_id,
            encrypted_content, created_at, federated_status, content_nonce)
           VALUES ($1, $2, $3, $4, $5, $6, NULL, $7)"#,
    )
    .bind(&msg.message_id)
    .bind(&msg.conversation_id)
    .bind(&msg.sender_id)
    .bind(&msg.remote_sender_qualified_id)
    .bind(&msg.encrypted_content)
    .bind(&msg.created_at)
    .bind(&msg.content_nonce)
    .execute(pool(&state))
    .await
    {
        Ok(r) if r.rows_affected() == 0 => {
            // Already stored, idempotent OK so sender marks delivered.
            info!(
                "s2s_receive_message: duplicate delivery {} (already stored)",
                message_id
            );
            let _ = repo::update_last_seen(pool(&state), &conn.id).await;
            return StatusCode::OK.into_response();
        }
        Ok(_) => {}
        Err(e) => {
            error!("s2s_receive_message: store message failed: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    // Spawn stub creation on a fresh task to avoid bloating this handler's future.
    let local_attachment_ids = tokio::spawn(create_message_media_stubs(
        pool(&state).clone(),
        msg_repo.clone(),
        inner.attachments.clone(),
        conn.address.clone(),
        message_id.clone(),
    ))
    .await
    .unwrap_or_default();

    if let Err(e) = msg_repo
        .create_deliveries(&message_id, &[recipient_id.clone()])
        .await
    {
        error!("s2s_receive_message: create delivery failed: {}", e);
    }

    // Notify the recipient via WebSocket.
    let ws_msg = serde_json::json!({
    "type": "new_message",
    "conversation_id": conversation_id,
    "message": {
    "message_id": message_id,
    "conversation_id": conversation_id,
    "sender_id": "__fed__",
    "remote_sender_qualified_id": envelope.sender_qualified_id,
    "encrypted_content": inner.encrypted_content,
    "attachments": local_attachment_ids,
    "created_at": inner.sent_at,
    }
    });
    crate::handlers::ws::send_to_user(&registry, &recipient_id, &ws_msg.to_string()).await;

    info!(
        "Stored federated message {} in conversation {} for user {}",
        message_id, conversation_id, recipient_id
    );
    let _ = repo::update_last_seen(pool(&state), &conn.id).await;
    StatusCode::OK.into_response()
}

// S2S inbound: post list

/// Serve this server's shareable posts to a peer.
/// POST /api/s2s/posts
pub async fn s2s_serve_posts(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let meta = match extract_meta(&headers) {
        Ok(m) => m,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let conn = match get_verified_connection(pool(&state), &meta, &body).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let enc = settings(&state)
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| ContentEncryption::from_hex_key(key).ok());

    let plaintext = match decrypt_body_if_needed(&headers, &body, &conn, enc.as_ref()) {
        Ok(b) => b,
        Err(resp) => return resp,
    };

    let req: S2SPostsRequest = serde_json::from_slice(&plaintext).unwrap_or(S2SPostsRequest {
        limit: 20,
        before: None,
    });
    let limit = req.limit.clamp(1, 100);
    let our_address = settings(&state).base_url.trim_end_matches('/').to_string();

    let posts = match repo::list_posts_for_peer(
        pool(&state),
        &conn.id,
        &our_address,
        limit,
        req.before.as_deref(),
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            error!("s2s_serve_posts: db error: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let has_more = posts.len() as i64 == limit;
    let _ = repo::update_last_seen(pool(&state), &conn.id).await;

    // Encrypt the response body.
    let resp_json = match serde_json::to_vec(&FederatedPostListResponse { posts, has_more }) {
        Ok(j) => j,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    match encrypt_response(&resp_json, &conn, enc.as_ref()) {
        Ok((ct_bytes, nonce_hex)) => axum::response::Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/plain; charset=utf-8")
            .header(HEADER_ENCRYPTED, "1")
            .header(HEADER_BODY_NONCE, nonce_hex)
            .body(axum::body::Body::from(ct_bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(resp) => resp,
    }
}

// User-facing: federated feed

#[derive(Debug, Deserialize)]
pub struct FederatedFeedQuery {
    #[serde(default = "default_fed_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_fed_limit() -> i64 {
    20
}

/// Return cached federated posts to the authenticated local user.
/// GET /api/posts/federated
pub async fn get_federated_feed(
    State(state): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Query(query): Query<FederatedFeedQuery>,
) -> impl IntoResponse {
    use magnolia_common::schemas::{PostContentResponse, PostListResponse, PostSummaryResponse};

    let user_id = &auth.user.user_id;
    let limit = query.limit.clamp(1, 100);
    let refs = match repo::list_all_post_refs(pool(&state), limit, query.offset).await {
        Ok(r) => r,
        Err(e) => {
            error!("get_federated_feed: db error: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let has_more = refs.len() as i64 == limit;
    let mut posts = Vec::new();

    for r in refs {
        // Skip posts from users the local user has externally banned.
        match repo::is_external_user_banned(
            pool(&state),
            user_id,
            &r.server_connection_id,
            &r.remote_user_id,
        )
        .await
        {
            Ok(true) => continue,
            Ok(false) => {}
            Err(e) => {
                warn!("get_federated_feed: ban check failed: {}", e);
            }
        }

        let content_json = match r.content_json.as_deref() {
            Some(j) => j,
            None => continue,
        };
        let entry: super::models::FederatedPostEntry = match serde_json::from_str(content_json) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let contents: Vec<PostContentResponse> = entry
            .contents
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                // media_ref.media_id was rewritten to the local stub id during ingest.
                let thumbnail_url = c
                    .media_ref
                    .as_ref()
                    .map(|m| format!("/api/media/{}/thumbnail", m.media_id));
                PostContentResponse {
                    content_id: format!("fed-{}-{}", r.remote_post_id, i),
                    content_type: c.content_type,
                    display_order: c.display_order,
                    content: c.content,
                    thumbnail_url,
                    filename: c.filename,
                    mime_type: c.mime_type,
                    file_size: c.file_size,
                }
            })
            .collect();

        posts.push(PostSummaryResponse {
            post_id: format!("fed:{}:{}", r.server_connection_id, r.remote_post_id),
            author_id: format!("fed:{}:{}", r.server_connection_id, r.remote_user_id),
            author_name: entry.author_name,
            author_avatar_url: entry.author_avatar_url,
            contents,
            tags: entry.tags,
            is_published: true,
            comment_count: 0,
            created_at: r.posted_at,
            source_server: r.server_address,
        });
    }

    let total = posts.len() as i64 + query.offset;
    Json(PostListResponse {
        posts,
        total,
        has_more,
        next_cursor: None,
    })
    .into_response()
}

// S2S inbound: discovery

pub async fn s2s_receive_discovery(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<Arc<ServerIdentity>>,
    axum::Extension(s2s_client): axum::Extension<client::S2SClient>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let meta = match extract_meta(&headers) {
        Ok(m) => m,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let conn = match get_verified_connection(pool(&state), &meta, &body).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };

    let enc = settings(&state)
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| ContentEncryption::from_hex_key(key).ok());

    let plaintext = match decrypt_body_if_needed(&headers, &body, &conn, enc.as_ref()) {
        Ok(b) => b,
        Err(resp) => return resp,
    };

    let push: DiscoveryPush = match serde_json::from_slice(&plaintext) {
        Ok(p) => p,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    for address in &push.known_servers {
        let _ = repo::upsert_discovery_hint(pool(&state), &conn.id, address).await;
    }

    let _ = repo::update_last_seen(pool(&state), &conn.id).await;

    // Relay the hints to other peers if relay_depth >= 2.
    let pool_clone = pool(&state).clone();
    let settings_clone = Arc::clone(settings(&state));
    let conn_id = conn.id.clone();
    let hints = push.known_servers.clone();
    tokio::spawn(super::relay::relay_received_hints(
        pool_clone,
        settings_clone,
        identity,
        s2s_client,
        hints,
        conn_id,
    ));

    StatusCode::OK.into_response()
}

// S2S: ECDH key lookup

/// Peer server requests a local user's ECDH public key for E2E setup.
/// Returns the key signed with this server's ML-DSA key.
/// GET /api/s2s/users/identity/{username}
///
/// Returns profile data for a single user by username so a receiving server can
/// verify an unknown message sender. Only returns users who have opted in to
/// federation sharing (sharing_mode != 'off').
pub async fn s2s_get_user_identity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(username): Path<String>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let meta = match extract_meta(&headers) {
        Ok(m) => m,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let conn = match get_verified_connection(pool(&state), &meta, &body).await {
        Ok(c) => c,
        Err(_) => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let enc = settings(&state)
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| ContentEncryption::from_hex_key(key).ok());

    // Only expose users who have opted into federation sharing.
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT ua.user_id, ua.username, ua.display_name, ua.avatar_media_id, ua.public_key
 FROM user_accounts ua
 JOIN user_federation_settings ufs ON ufs.user_id = ua.user_id
 WHERE ua.username = $1
 AND ua.active = 1
 AND ua.user_id != '__fed__'
 AND ua.user_id != '__proxy__'
 AND ufs.sharing_mode != 'off'",
    )
    .bind(&username)
    .fetch_optional(pool(&state))
    .await;

    let row = match row {
        Ok(Some(r)) => r,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            error!("s2s_get_user_identity: db error: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let user_id: String = row.get("user_id");
    let username: String = row.get::<Option<String>, _>("username").unwrap_or_default();
    let display_name: Option<String> = row.get("display_name");
    let avatar_media_id: Option<String> = row.get("avatar_media_id");
    let ecdh_public_key: Option<String> = row.get("public_key");

    let avatar_url = avatar_media_id.map(|id| format!("/api/media/{}/thumbnail", id));

    let resp_json = match serde_json::to_vec(&UserIdentityResponse {
        remote_user_id: user_id,
        username,
        display_name,
        avatar_url,
        ecdh_public_key,
    }) {
        Ok(j) => j,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    match encrypt_response(&resp_json, &conn, enc.as_ref()) {
        Ok((ct_bytes, nonce_hex)) => axum::response::Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/plain; charset=utf-8")
            .header(HEADER_ENCRYPTED, "1")
            .header(HEADER_BODY_NONCE, nonce_hex)
            .body(axum::body::Body::from(ct_bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(resp) => resp,
    }
}

pub async fn s2s_get_ecdh_key(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<Arc<ServerIdentity>>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let meta = match extract_meta(&headers) {
        Ok(m) => m,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    // Verify the request came from a known active peer.
    if get_verified_connection(pool(&state), &meta, &body)
        .await
        .is_err()
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // Look up the user's ECDH public key.
    let ecdh_key: Option<String> =
        sqlx::query_scalar("SELECT public_key FROM user_accounts WHERE user_id = $1")
            .bind(&user_id)
            .fetch_optional(pool(&state))
            .await
            .unwrap_or(None)
            .flatten();

    let ecdh_key = match ecdh_key {
        Some(k) => k,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    // Sign (user_id || ecdh_key) so the receiver can verify authenticity.
    use ml_dsa::signature::Signer;
    let signing_key = identity.signing_key();
    let msg = format!("{}{}", user_id, ecdh_key);
    let sig = signing_key.sign(msg.as_bytes());
    let sig_hex = hex::encode(sig.encode().as_slice());

    Json(super::models::EcdhKeyResponse {
        remote_user_id: user_id,
        ecdh_public_key: ecdh_key,
        signature: sig_hex,
    })
    .into_response()
}

// User-facing: remote user search

pub async fn search_remote_users(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let query = params
        .get("q")
        .map(String::as_str)
        .unwrap_or("")
        .to_string();
    let limit: i64 = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(40)
        .min(100);
    let offset: i64 = params
        .get("offset")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let server_id = params.get("server_id").cloned();

    // Fetch all active connections for address lookup.
    let connections = repo::list_active_connections(pool(&state))
        .await
        .unwrap_or_default();
    let addr_map: std::collections::HashMap<String, String> =
        connections.into_iter().map(|c| (c.id, c.address)).collect();

    let rows_result = match &server_id {
        Some(sid) => {
            repo::search_federation_users_for_server(pool(&state), sid, &query, limit, offset).await
        }
        None => repo::search_federation_users(pool(&state), &query, limit, offset).await,
    };

    match rows_result {
        Ok(rows) => {
            let dtos: Vec<FederationUserDto> = rows
                .into_iter()
                .map(|row| {
                    let addr = addr_map
                        .get(&row.server_connection_id)
                        .map(String::as_str)
                        .unwrap_or("");
                    FederationUserDto::from_row_and_address(row, addr)
                })
                .collect();
            Json(dtos).into_response()
        }
        Err(e) => {
            error!("Federation user search failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// User-facing: server list

pub async fn list_servers_for_user(State(state): State<AppState>) -> impl IntoResponse {
    match repo::list_active_connections(pool(&state)).await {
        Ok(conns) => {
            let items: Vec<_> = conns
                .into_iter()
                .map(|c| {
                    serde_json::json!({
                    "id": c.id,
                    "address": c.address,
                    "display_name": c.display_name,
                    })
                })
                .collect();
            Json(serde_json::json!({ "servers": items })).into_response()
        }
        Err(e) => {
            error!("DB error listing active servers: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// User-facing: federation rules (whitelist/blacklist per server)

pub async fn list_my_federation_rules(
    State(state): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
) -> impl IntoResponse {
    match repo::list_federation_rules(pool(&state), &auth.user.user_id).await {
        Ok(rows) => {
            let items: Vec<_> = rows
                .into_iter()
                .map(|r| {
                    serde_json::json!({
                    "server_connection_id": r.server_connection_id,
                    "rule_type": r.rule_type,
                    "effect": r.effect,
                    })
                })
                .collect();
            Json(serde_json::json!({ "rules": items })).into_response()
        }
        Err(e) => {
            error!("DB error listing federation rules: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AddFederationRuleBody {
    pub server_connection_id: String,
    pub rule_type: String,
    pub effect: String,
}

pub async fn add_my_federation_rule(
    State(state): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Json(body): Json<AddFederationRuleBody>,
) -> impl IntoResponse {
    let valid_rule_types = ["sharing", "post_sharing"];
    let valid_effects = ["allow", "deny"];
    if !valid_rule_types.contains(&body.rule_type.as_str())
        || !valid_effects.contains(&body.effect.as_str())
    {
        return (StatusCode::BAD_REQUEST, "Invalid rule_type or effect").into_response();
    }
    match repo::upsert_federation_rule(
        pool(&state),
        &auth.user.user_id,
        &body.server_connection_id,
        &body.rule_type,
        &body.effect,
    )
    .await
    {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => {
            error!("DB error upserting federation rule: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn remove_my_federation_rule(
    State(state): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path((server_id, rule_type)): Path<(String, String)>,
) -> impl IntoResponse {
    match repo::delete_federation_rule(pool(&state), &auth.user.user_id, &server_id, &rule_type)
        .await
    {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => {
            error!("DB error removing federation rule: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// User-facing: federation settings

pub async fn get_my_federation_settings(
    State(state): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
) -> impl IntoResponse {
    match repo::get_user_federation_settings(pool(&state), &auth.user.user_id).await {
        Ok(Some(row)) => Json(serde_json::json!({
        "sharing_mode": row.sharing_mode,
        "post_sharing_mode": row.post_sharing_mode,
        }))
        .into_response(),
        Ok(None) => Json(serde_json::json!({
        "sharing_mode": "off",
        "post_sharing_mode": "off",
        }))
        .into_response(),
        Err(e) => {
            error!("DB error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateFederationSettingsBody {
    pub sharing_mode: String,
    pub post_sharing_mode: String,
}

pub async fn update_my_federation_settings(
    State(state): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    axum::Extension(identity): axum::Extension<Arc<ServerIdentity>>,
    axum::Extension(s2s_client): axum::Extension<client::S2SClient>,
    Json(body): Json<UpdateFederationSettingsBody>,
) -> impl IntoResponse {
    let valid = ["off", "whitelist", "blacklist"];
    if !valid.contains(&body.sharing_mode.as_str())
        || !valid.contains(&body.post_sharing_mode.as_str())
    {
        return (StatusCode::BAD_REQUEST, "Invalid sharing mode").into_response();
    }

    match repo::upsert_user_federation_settings(
        pool(&state),
        &auth.user.user_id,
        &body.sharing_mode,
        &body.post_sharing_mode,
    )
    .await
    {
        Ok(_) => {
            // Fan out the updated user list to all peers in the background.
            let pool_clone = pool(&state).clone();
            let settings_clone = Arc::clone(settings(&state));
            tokio::spawn(super::sync::push_user_list_to_all_peers(
                pool_clone,
                settings_clone,
                identity,
                s2s_client,
            ));
            StatusCode::OK.into_response()
        }
        Err(e) => {
            error!("DB error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// User-facing: external bans

#[derive(Debug, Deserialize)]
pub struct AddExternalBanBody {
    pub server_connection_id: String,
    pub remote_user_id: String,
}

pub async fn list_external_bans(
    State(state): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
) -> impl IntoResponse {
    match repo::list_external_bans_enriched(pool(&state), &auth.user.user_id).await {
        Ok(rows) => {
            let items: Vec<_> = rows
                .into_iter()
                .map(|r| {
                    serde_json::json!({
                    "server_connection_id": r.server_connection_id,
                    "remote_user_id": r.remote_user_id,
                    "banned_at": r.banned_at,
                    "server_address": r.server_address,
                    "server_display_name": r.server_display_name,
                    "user_display_name": r.user_display_name,
                    "username": r.username,
                    })
                })
                .collect();
            Json(serde_json::json!({ "bans": items })).into_response()
        }
        Err(e) => {
            error!("DB error listing external bans: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn add_external_ban(
    State(state): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Json(body): Json<AddExternalBanBody>,
) -> impl IntoResponse {
    match repo::insert_external_ban(
        pool(&state),
        &auth.user.user_id,
        &body.server_connection_id,
        &body.remote_user_id,
    )
    .await
    {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => {
            error!("DB error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn remove_external_ban(
    State(state): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path((server_id, remote_user_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match repo::delete_external_ban(
        pool(&state),
        &auth.user.user_id,
        &server_id,
        &remote_user_id,
    )
    .await
    {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => {
            error!("DB error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// Internal helpers

/// Look up an active connection by the sender address from S2S headers and
/// verify the request signature. Returns the connection row on success.
/// GET /api/s2s/ws — WebSocket upgrade for the persistent S2S hub.
///
/// No S2S header auth here; authentication happens inside the WS handshake
/// via a signed Hello message (see [`hub::handle_peer_ws`]).
pub async fn s2s_ws_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Extension(identity): Extension<Arc<ServerIdentity>>,
    Extension(registry): Extension<super::hub::HubRegistry>,
    Extension(hub_status): Extension<super::hub::PeerStatusMap>,
) -> impl IntoResponse {
    let pool = pool(&state).clone();
    let settings = Arc::clone(settings(&state));
    ws.on_upgrade(move |socket| {
        super::hub::handle_peer_ws(socket, pool, settings, identity, registry, hub_status)
    })
}

/// GET /api/admin/federation/hub-status
/// Returns the live WS connection status for all peer connections.
pub async fn admin_get_hub_status() -> impl IntoResponse {
    let statuses = super::hub::get_peer_statuses().await;
    Json(serde_json::json!({ "peers": statuses }))
}

/// Decrypt an incoming S2S request body if `X-Magnolia-Encrypted: 1` is present.
///
/// Returns the plaintext bytes (or the original bytes unchanged if not encrypted).
/// `conn` must have a `shared_secret` with the other server, returns `Err(response)` if decryption fails.
fn decrypt_body_if_needed(
    headers: &HeaderMap,
    body: &axum::body::Bytes,
    conn: &super::models::ServerConnectionRow,
    enc: Option<&ContentEncryption>,
) -> Result<Vec<u8>, axum::response::Response> {
    let is_encrypted = headers
        .get(HEADER_ENCRYPTED)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "1")
        .unwrap_or(false);

    if !is_encrypted {
        return Ok(body.to_vec());
    }

    let nonce_hex = headers
        .get(HEADER_BODY_NONCE)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| StatusCode::BAD_REQUEST.into_response())?
        .to_string();

    let raw_secret = conn
        .shared_secret
        .as_deref()
        .ok_or_else(|| StatusCode::INTERNAL_SERVER_ERROR.into_response())?;

    let shared_secret = load_shared_secret(raw_secret, enc)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())?;

    let ct_b64 = std::str::from_utf8(body).map_err(|_| StatusCode::BAD_REQUEST.into_response())?;

    handshake::decrypt_s2s_payload(&shared_secret, ct_b64, &nonce_hex)
        .map_err(|_| StatusCode::BAD_REQUEST.into_response())
}

/// Encrypt a response body and set the appropriate headers.
fn encrypt_response(
    body: &[u8],
    conn: &super::models::ServerConnectionRow,
    enc: Option<&ContentEncryption>,
) -> Result<(Vec<u8>, String), axum::response::Response> {
    let raw_secret = conn
        .shared_secret
        .as_deref()
        .ok_or_else(|| StatusCode::INTERNAL_SERVER_ERROR.into_response())?;

    let shared_secret = load_shared_secret(raw_secret, enc)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())?;

    let (ct_b64, nonce_hex) = handshake::encrypt_s2s_payload(&shared_secret, body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())?;

    Ok((ct_b64.into_bytes(), nonce_hex))
}

async fn get_verified_connection(
    pool: &AnyPool,
    meta: &super::models::S2SRequestMeta,
    body: &[u8],
) -> Result<super::models::ServerConnectionRow, axum::response::Response> {
    let sender_address = meta.sender_address.trim_end_matches('/').to_string();

    // Fast path: look up by the address the peer declared in the header.
    if let Ok(Some(conn)) = repo::get_connection_by_address(pool, &sender_address).await {
        if conn.status == ConnectionStatus::Active.as_str() {
            if let Some(dsa_bytes) = conn.their_ml_dsa_public_key.as_deref() {
                if let Ok(vk) = decode_verifying_key(dsa_bytes) {
                    if verify_request(body, meta, &vk).is_ok() {
                        return Ok(conn);
                    }
                }
            }
        }
    }

    // Slow path: the peer may be calling from a different address (VPN, IP change, etc).
    // Scan all active connections and try each key until one verifies the signature.
    // If found, update the stored address to the one the peer is currently using.
    let active = repo::list_active_connections(pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())?;

    for conn in active {
        let dsa_bytes = match conn.their_ml_dsa_public_key.as_deref() {
            Some(b) => b,
            None => continue,
        };
        let vk = match decode_verifying_key(dsa_bytes) {
            Ok(k) => k,
            Err(_) => continue,
        };
        if verify_request(body, meta, &vk).is_ok() {
            // Signature verified, peer is known but calling from a new address.
            // Update the stored address so the fast path works next time.
            // This can happen because of physical server move, different ISP, or VPN.
            // Not necessarily malicious, but TODO: come up with counter measure.
            if conn.address != sender_address {
                info!(
                    "get_verified_connection: peer {} now reachable at '{}' (was '{}'), updating",
                    conn.id, sender_address, conn.address
                );
                let _ = repo::update_connection_address(pool, &conn.id, &sender_address).await;
            }
            return Ok(conn);
        }
    }

    Err(StatusCode::UNAUTHORIZED.into_response())
}

// User-facing: start federated DM

#[derive(Deserialize)]
pub struct StartFederatedDmRequest {
    pub server_connection_id: String,
    pub remote_user_id: String,
}

/// POST /api/federation/dm
/// Find or create a DM conversation with a remote federated user.
pub async fn start_federated_dm(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Json(payload): Json<StartFederatedDmRequest>,
) -> impl IntoResponse {
    // Verify the connection exists and is active.
    let conn = match repo::get_connection_by_id(pool(&state), &payload.server_connection_id).await {
        Ok(Some(c)) if c.status == ConnectionStatus::Active.as_str() => c,
        Ok(_) => {
            return (
                StatusCode::BAD_REQUEST,
                "Connection not found or not active",
            )
                .into_response();
        }
        Err(e) => {
            error!("start_federated_dm: db error: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Verify the remote user is known in our federation_users table.
    let remote_user = match repo::get_federation_user(
        pool(&state),
        &payload.server_connection_id,
        &payload.remote_user_id,
    )
    .await
    {
        Ok(Some(u)) => u,
        Ok(None) => return (StatusCode::NOT_FOUND, "Remote user not found").into_response(),
        Err(e) => {
            error!("start_federated_dm: db error: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let host = conn
        .address
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    let remote_ident = if remote_user.username.is_empty() {
        &remote_user.remote_user_id
    } else {
        &remote_user.username
    };
    let remote_qualified_id = format!("{}@{}", remote_ident, host);

    // Find existing or create new DM.
    let conv_id = match repo::find_federated_dm(
        pool(&state),
        &auth.user.user_id,
        &payload.server_connection_id,
        &payload.remote_user_id,
    )
    .await
    {
        Ok(Some(id)) => id,
        Ok(None) => {
            match repo::create_federated_dm(
                pool(&state),
                &auth.user.user_id,
                &payload.server_connection_id,
                &payload.remote_user_id,
                &remote_qualified_id,
            )
            .await
            {
                Ok(id) => id,
                Err(e) => {
                    error!("start_federated_dm: create failed: {}", e);
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
        }
        Err(e) => {
            error!("start_federated_dm: find failed: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let display_name = remote_user
        .display_name
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| remote_qualified_id.clone());
    Json(serde_json::json!({
    "conversation_id": conv_id,
    "display_name": display_name,
    }))
    .into_response()
}

// S2S media serving

#[derive(serde::Deserialize)]
pub struct S2SMediaQuery {
    /// The remote user_id on the requesting server who triggered the fetch.
    /// Used to enforce the media owner's block list and external bans.
    pub remote_user_id: String,
}

/// GET /api/s2s/media/{media_id}?remote_user_id={remote_user_id}
///
/// Streams a local media file to a verified peer server.
/// Access control:
/// 1. ML-DSA signature (handled by route middleware / get_verified_connection).
/// 2. Connection must be active.
/// 3. If the media owner has externally banned the requesting remote user, returns 403.
/// 4. Only serves fully-cached, non-deleted, local-origin media.
pub async fn s2s_serve_media_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(media_id): Path<String>,
    Query(query): Query<S2SMediaQuery>,
    body: axum::body::Bytes,
) -> Response {
    // Verify the requesting peer
    let meta = match extract_meta(&headers) {
        Ok(m) => m,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let conn = match get_verified_connection(pool(&state), &meta, &body).await {
        Ok(c) => c,
        Err(r) => return r,
    };

    let pool = pool(&state);
    let settings = settings(&state);

    let repo = magnolia_common::repositories::MediaRepository::new(pool.clone());

    // Load the media row, must be local, cached, and not deleted
    let media = match repo.get_by_id(&media_id).await {
        Ok(Some(m)) => m,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            error!("s2s_serve_media_file: db error: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Only serve locally-owned, fully-cached files
    if media.origin_server.is_some() || media.is_cached == 0 {
        return StatusCode::NOT_FOUND.into_response();
    }

    // Check if the media owner has externally banned the requesting remote user
    let banned = repo
        .is_externally_banned(&media.owner_id, &conn.id, &query.remote_user_id)
        .await
        .unwrap_or(false);
    if banned {
        return StatusCode::FORBIDDEN.into_response();
    }

    // Read and optionally decrypt the file
    let enc = settings
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| crate::utils::encryption::ContentEncryption::from_hex_key(key).ok());

    let file_data = match tokio::fs::read(&media.storage_path).await {
        Ok(d) => d,
        Err(e) => {
            error!("s2s_serve_media_file: read error: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let file_data = if let (Some(nonce_hex), Some(enc)) = (&media.encryption_nonce, enc) {
        let nonce_bytes = match hex::decode(nonce_hex) {
            Ok(b) => b,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        match enc.decrypt_content(&file_data, &nonce_bytes) {
            Ok(d) => d,
            Err(e) => {
                error!("s2s_serve_media_file: decrypt error: {:?}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    } else {
        file_data
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, &media.mime_type)
        .header(header::CONTENT_LENGTH, file_data.len())
        .header(header::CACHE_CONTROL, "private, no-store")
        .body(Body::from(file_data))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
