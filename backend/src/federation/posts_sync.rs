//! Background sync of remote posts into `federation_post_refs`.
//!
//! Triggered fire-and-forget when a connection becomes active.

use sqlx::AnyPool;
use std::sync::Arc;
use tracing::{error, info, warn};

use super::{
    client::{self, S2SClient},
    identity::{ServerIdentity, load_shared_secret},
    models::{FederatedPostEntry, S2SPostsRequest},
    repo,
};
use crate::{config::Settings, utils::encryption::ContentEncryption};
use magnolia_common::repositories::MediaRepository;

/// Compute SHA-256 content hash from the serialised contents array.
fn compute_content_hash(entry: &FederatedPostEntry) -> String {
    use sha2::{Digest, Sha256};
    let json = serde_json::to_string(&entry.contents).unwrap_or_default();
    hex::encode(Sha256::digest(json.as_bytes()))
}

/// Fetch posts from one peer and upsert into `federation_post_refs`.
pub async fn fetch_and_store_posts_from_peer(
    pool: AnyPool,
    settings: Arc<Settings>,
    identity: Arc<ServerIdentity>,
    client: S2SClient,
    peer_address: String,
    peer_conn_id: String,
) {
    let enc = settings
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| ContentEncryption::from_hex_key(key).ok());

    let conn = match repo::get_connection_by_id(&pool, &peer_conn_id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            warn!(
                "fetch_posts_from_peer: connection {} not found",
                peer_conn_id
            );
            return;
        }
        Err(e) => {
            warn!("fetch_posts_from_peer: db error: {}", e);
            return;
        }
    };

    let shared_secret = match conn.shared_secret.as_deref() {
        Some(raw) => match load_shared_secret(raw, enc.as_ref()) {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    "fetch_posts_from_peer: failed to load shared secret for {}: {}",
                    peer_address, e
                );
                return;
            }
        },
        None => {
            warn!(
                "fetch_posts_from_peer: no shared secret for {}, skipping",
                peer_address
            );
            return;
        }
    };

    let our_address = settings.base_url.trim_end_matches('/').to_string();
    let req = S2SPostsRequest {
        limit: 50,
        before: None,
    };

    let response = match client::fetch_posts(
        &client,
        &identity,
        &our_address,
        &peer_address,
        &req,
        &shared_secret,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("fetch_posts_from_peer({}): {}", peer_address, e);
            return;
        }
    };

    info!(
        "fetch_posts_from_peer({}): received {} posts",
        peer_address,
        response.posts.len()
    );

    let media_repo = MediaRepository::new(pool.clone());
    let now_str = chrono::Utc::now().to_rfc3339();

    for mut entry in response.posts {
        let content_hash = compute_content_hash(&entry);

        // Create local stubs for any media refs and rewrite to local URLs so the
        // feed handler and clients can resolve media without knowing the remote address.
        for c in &mut entry.contents {
            if let Some(ref mut att) = c.media_ref {
                let local_id = match media_repo
                    .find_by_origin(&peer_address, &att.media_id)
                    .await
                {
                    Ok(Some(existing)) => existing.media_id,
                    _ => {
                        let new_id = uuid::Uuid::new_v4().to_string();
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
                            warn!("fetch_posts_from_peer: create media stub failed: {}", e);
                            continue;
                        }
                        new_id
                    }
                };
                // Replace the remote media_id with the local stub id.
                // The frontend constructs /api/media/{id}/file and /api/media/{id}/thumbnail
                // from the raw media_id, so content must be just the UUID.
                att.media_id = local_id.clone();
                c.content = local_id.clone();
            }
        }

        let content_json = match serde_json::to_string(&entry) {
            Ok(j) => j,
            Err(e) => {
                error!("fetch_posts_from_peer: serialize entry: {}", e);
                continue;
            }
        };
        if let Err(e) = repo::upsert_post_ref(
            &pool,
            &peer_conn_id,
            &entry.user_id,
            &entry.post_id,
            &entry.posted_at,
            &content_hash,
            &content_json,
            &peer_address,
        )
        .await
        {
            error!("fetch_posts_from_peer: upsert failed: {}", e);
        }
    }
}

/// Fan out to all active peers and fetch their post lists.
pub async fn sync_posts_for_all_peers(
    pool: AnyPool,
    settings: Arc<Settings>,
    identity: Arc<ServerIdentity>,
    client: S2SClient,
) {
    let connections = match repo::list_active_connections(&pool).await {
        Ok(c) => c,
        Err(e) => {
            error!("sync_posts_for_all_peers: {}", e);
            return;
        }
    };

    for conn in connections {
        let pool_c = pool.clone();
        let settings_c = Arc::clone(&settings);
        let identity_c = Arc::clone(&identity);
        let client_c = client.clone();
        tokio::spawn(fetch_and_store_posts_from_peer(
            pool_c,
            settings_c,
            identity_c,
            client_c,
            conn.address,
            conn.id,
        ));
    }
}
