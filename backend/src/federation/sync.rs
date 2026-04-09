//! User-list sync helpers.
//!
//! Builds the [`UserListSync`] payload from the local database and fans it out
//! to all active peers (or a single specified peer) as a fire-and-forget
//! background task.

use std::sync::Arc;
use tracing::{info, warn};

use super::{
    client::{self, S2SClient},
    identity::{ServerIdentity, load_shared_secret},
    models::{FederatedUserEntry, UserListSync},
    repo,
};
use crate::{config::Settings, utils::encryption::ContentEncryption};
use sqlx::AnyPool;

/// Build the [`UserListSync`] payload from locally opted-in users.
///
/// `base_url` is used to construct absolute avatar URLs.
async fn build_payload(pool: &AnyPool, base_url: &str) -> UserListSync {
    let base = base_url.trim_end_matches('/');
    let users = match repo::list_shareable_users(pool).await {
        Ok(u) => u,
        Err(e) => {
            warn!("Failed to query shareable users for sync: {}", e);
            return UserListSync { users: vec![] };
        }
    };

    let entries = users
        .into_iter()
        .map(|u| FederatedUserEntry {
            remote_user_id: u.user_id,
            username: u.username,
            display_name: u.display_name,
            avatar_url: u
                .avatar_media_id
                .map(|id| format!("{}/api/media/{}/thumbnail", base, id)),
            ecdh_public_key: u.ecdh_public_key,
        })
        .collect();

    UserListSync { users: entries }
}

/// Push the local shareable user list to **all** active peers.
///
/// Intended to be called inside `tokio::spawn` — all errors are logged and
/// swallowed so they never propagate to the caller.
pub async fn push_user_list_to_all_peers(
    pool: AnyPool,
    settings: Arc<Settings>,
    identity: Arc<ServerIdentity>,
    client: S2SClient,
) {
    let base_url = settings.base_url.trim_end_matches('/').to_string();
    let payload = build_payload(&pool, &base_url).await;

    let enc = settings
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| ContentEncryption::from_hex_key(key).ok());

    let connections = match repo::list_active_connections(&pool).await {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to query active connections for user sync: {}", e);
            return;
        }
    };

    info!(
        "Pushing user list ({} users) to {} peers",
        payload.users.len(),
        connections.len()
    );

    for conn in connections {
        let shared_secret = match conn.shared_secret.as_deref() {
            Some(raw) => match load_shared_secret(raw, enc.as_ref()) {
                Ok(s) => s,
                Err(e) => {
                    warn!(
                        "User sync: failed to load shared secret for {}: {}",
                        conn.address, e
                    );
                    continue;
                }
            },
            None => {
                warn!("User sync: no shared secret for {}, skipping", conn.address);
                continue;
            }
        };

        let payload_clone = payload.clone();
        let client_clone = client.clone();
        let identity_clone = Arc::clone(&identity);
        let base_url_clone = base_url.clone();
        let peer_address = conn.address.clone();
        tokio::spawn(async move {
            if let Err(e) = client::send_user_list_sync(
                &client_clone,
                &identity_clone,
                &base_url_clone,
                &peer_address,
                payload_clone,
                &shared_secret,
            )
            .await
            {
                warn!("User list sync to {} failed: {}", peer_address, e);
            } else {
                info!("User list sync delivered to {}", peer_address);
            }
        });
    }
}

/// Push the local shareable user list to a **single** peer by connection id.
///
/// Used after a new connection becomes active so the peer gets our list
/// immediately without waiting for the next scheduled sync.
pub async fn push_user_list_to_peer(
    pool: AnyPool,
    settings: Arc<Settings>,
    identity: Arc<ServerIdentity>,
    client: S2SClient,
    peer_address: String,
    peer_conn_id: String,
) {
    let base_url = settings.base_url.trim_end_matches('/').to_string();
    let payload = build_payload(&pool, &base_url).await;

    let enc = settings
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| ContentEncryption::from_hex_key(key).ok());

    let conn = match repo::get_connection_by_id(&pool, &peer_conn_id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            warn!(
                "push_user_list_to_peer: connection {} not found",
                peer_conn_id
            );
            return;
        }
        Err(e) => {
            warn!("push_user_list_to_peer: db error: {}", e);
            return;
        }
    };

    let shared_secret = match conn.shared_secret.as_deref() {
        Some(raw) => match load_shared_secret(raw, enc.as_ref()) {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    "push_user_list_to_peer: failed to load shared secret for {}: {}",
                    peer_address, e
                );
                return;
            }
        },
        None => {
            warn!(
                "push_user_list_to_peer: no shared secret for {}",
                peer_address
            );
            return;
        }
    };

    if let Err(e) = client::send_user_list_sync(
        &client,
        &identity,
        &base_url,
        &peer_address,
        payload,
        &shared_secret,
    )
    .await
    {
        warn!("Initial user list sync to {} failed: {}", peer_address, e);
    } else {
        info!("Initial user list sync delivered to {}", peer_address);
    }
}
