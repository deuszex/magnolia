//! Server discovery relay.
//!
//! Two trigger points:
//!
//! 1. **New connection becomes active** — [`push_discovery_on_connect`]
//! If `federation_share_connections` is enabled and `relay_depth >= 1`,
//! push the list of our shareable peer addresses to the new peer (provided
//! they set `wants_discovery`).
//!
//! 2. **Inbound discovery push received** — [`relay_received_hints`]
//! After storing the received addresses as local hints, if `relay_depth >= 2`,
//! forward them to every other active peer that wants discovery (excluding
//! the sender). This propagates hints one more hop through the network.

use std::sync::Arc;
use tracing::{info, warn};

use super::{
    client::{self, S2SClient},
    identity::{ServerIdentity, load_shared_secret},
    models::DiscoveryPush,
    repo,
};
use crate::{config::Settings, utils::encryption::ContentEncryption};
use sqlx::AnyPool;

// Site-config helpers

struct RelayConfig {
    share_connections: bool,
    relay_depth: i64,
}

async fn load_relay_config(pool: &AnyPool) -> RelayConfig {
    use sqlx::Row;
    match sqlx::query(
        "SELECT federation_share_connections, federation_relay_depth
 FROM site_config WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
    {
        Ok(Some(r)) => RelayConfig {
            share_connections: r.get::<i32, _>("federation_share_connections") != 0,
            relay_depth: r.get::<i64, _>("federation_relay_depth"),
        },
        _ => RelayConfig {
            share_connections: false,
            relay_depth: 0,
        },
    }
}

// Public API

/// Called (inside `tokio::spawn`) when a new connection becomes active.
///
/// If the site has `federation_share_connections` enabled and `relay_depth >= 1`,
/// and the new peer set `wants_discovery`, sends a [`DiscoveryPush`] containing
/// the addresses of our other shareable active peers.
pub async fn push_discovery_on_connect(
    pool: AnyPool,
    settings: Arc<Settings>,
    identity: Arc<ServerIdentity>,
    client: S2SClient,
    new_peer_address: String,
    new_peer_wants_discovery: bool,
    new_peer_conn_id: String,
) {
    if !new_peer_wants_discovery {
        return;
    }

    let cfg = load_relay_config(&pool).await;
    if !cfg.share_connections || cfg.relay_depth < 1 {
        return;
    }

    let enc = settings
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| ContentEncryption::from_hex_key(key).ok());

    let conn = match repo::get_connection_by_id(&pool, &new_peer_conn_id).await {
        Ok(Some(c)) => c,
        _ => return,
    };

    let shared_secret = match conn.shared_secret.as_deref() {
        Some(raw) => match load_shared_secret(raw, enc.as_ref()) {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    "Discovery push: failed to load shared secret for {}: {}",
                    new_peer_address, e
                );
                return;
            }
        },
        None => {
            warn!(
                "Discovery push: no shared secret for {}, skipping",
                new_peer_address
            );
            return;
        }
    };

    // Collect shareable peer addresses, excluding the new peer itself.
    let shareable = match repo::list_shareable_connections(&pool).await {
        Ok(c) => c,
        Err(e) => {
            warn!(
                "Discovery relay: failed to list shareable connections: {}",
                e
            );
            return;
        }
    };

    let peer_norm = new_peer_address.trim_end_matches('/');
    let addresses: Vec<String> = shareable
        .into_iter()
        .map(|c| c.address)
        .filter(|a| a.trim_end_matches('/') != peer_norm)
        .collect();

    if addresses.is_empty() {
        return;
    }

    let our_address = settings.base_url.trim_end_matches('/').to_string();
    let count = addresses.len();
    let push = DiscoveryPush {
        known_servers: addresses,
    };

    if let Err(e) = client::send_discovery(
        &client,
        &identity,
        &our_address,
        &new_peer_address,
        push,
        &shared_secret,
    )
    .await
    {
        warn!("Discovery push to {} failed: {}", new_peer_address, e);
    } else {
        info!(
            "Discovery push sent to {} ({} addresses)",
            new_peer_address, count
        );
    }
}

/// Called (inside `tokio::spawn`) after inbound discovery hints are stored.
///
/// If `relay_depth >= 2`, forwards the received addresses to all other active
/// peers that want discovery (excluding the original sender's connection).
pub async fn relay_received_hints(
    pool: AnyPool,
    settings: Arc<Settings>,
    identity: Arc<ServerIdentity>,
    client: S2SClient,
    hints: Vec<String>,
    sender_conn_id: String,
) {
    if hints.is_empty() {
        return;
    }

    let cfg = load_relay_config(&pool).await;
    if cfg.relay_depth < 2 {
        return;
    }

    let enc = settings
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| ContentEncryption::from_hex_key(key).ok());

    // Get the sender's address so we can exclude it from forwarding targets.
    let sender_address = match repo::get_connection_by_id(&pool, &sender_conn_id).await {
        Ok(Some(c)) => c.address,
        _ => return,
    };

    // All active connections that want discovery, excluding the sender.
    let targets = match repo::list_active_connections(&pool).await {
        Ok(c) => c,
        Err(e) => {
            warn!("Discovery relay: failed to list active connections: {}", e);
            return;
        }
    };

    let sender_norm = sender_address.trim_end_matches('/');
    let targets: Vec<_> = targets
        .into_iter()
        .filter(|c| c.peer_wants_discovery != 0 && c.address.trim_end_matches('/') != sender_norm)
        .collect();

    if targets.is_empty() {
        return;
    }

    let our_address = settings.base_url.trim_end_matches('/').to_string();

    // Filter out addresses already known as active connections on this server
    // to avoid relaying stale or circular data.
    let known_addresses: std::collections::HashSet<String> =
        match repo::list_active_connections(&pool).await {
            Ok(conns) => conns
                .into_iter()
                .map(|c| c.address.trim_end_matches('/').to_string())
                .collect(),
            Err(_) => std::collections::HashSet::new(),
        };

    let filtered_hints: Vec<String> = hints
        .into_iter()
        .filter(|a| {
            let norm = a.trim_end_matches('/');
            norm != our_address.as_str() && !known_addresses.contains(norm)
        })
        .collect();

    if filtered_hints.is_empty() {
        return;
    }

    info!(
        "Relaying {} discovery hint(s) to {} peer(s)",
        filtered_hints.len(),
        targets.len()
    );

    for target in targets {
        let shared_secret = match target.shared_secret.as_deref() {
            Some(raw) => match load_shared_secret(raw, enc.as_ref()) {
                Ok(s) => s,
                Err(e) => {
                    warn!(
                        "Discovery relay: failed to load shared secret for {}: {}",
                        target.address, e
                    );
                    continue;
                }
            },
            None => {
                warn!(
                    "Discovery relay: no shared secret for {}, skipping",
                    target.address
                );
                continue;
            }
        };

        let push = DiscoveryPush {
            known_servers: filtered_hints.clone(),
        };
        let client_c = client.clone();
        let identity_c = Arc::clone(&identity);
        let our_addr_c = our_address.clone();
        let peer_addr = target.address.clone();
        tokio::spawn(async move {
            let _ = client::send_discovery(
                &client_c,
                &identity_c,
                &our_addr_c,
                &peer_addr,
                push,
                &shared_secret,
            )
            .await;
        });
    }
}
