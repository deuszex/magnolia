//! Outbound cross-server message delivery.
//!
//! Each peer delivery is a separate spawned task so no single future grows large
//! enough to overflow the tokio worker stack.

use sqlx::AnyPool;
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

use super::{
    client::{self, S2SClient},
    handshake::encrypt_s2s_payload,
    identity::{ServerIdentity, load_shared_secret},
    models::{FederatedMediaRef, S2SInnerMessage, S2SMessageEnvelope},
    repo,
};
use crate::config::Settings;
use crate::utils::encryption::ContentEncryption;
use magnolia_common::repositories::MessageRepository;

/// Called fire-and-forget after a local message is stored.
/// Enqueues an outbound delivery entry for each federated member then spawns
/// one task per peer so no individual future becomes large.
#[allow(clippy::too_many_arguments)]
pub async fn forward_message_to_peers(
    pool: AnyPool,
    settings: Arc<Settings>,
    identity: Arc<ServerIdentity>,
    client: S2SClient,
    conversation_id: String,
    conversation_type: String,
    group_name: Option<String>,
    message_id: String,
    encrypted_content: String,
    sender_id: String,
    sent_at: String,
    attachments: Vec<FederatedMediaRef>,
) {
    let members = match repo::list_federated_members(&pool, &conversation_id).await {
        Ok(m) => m,
        Err(e) => {
            error!("forward_message_to_peers: list members failed: {}", e);
            return;
        }
    };
    if members.is_empty() {
        return;
    }

    let enc = settings
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| ContentEncryption::from_hex_key(key).ok());

    let host = settings
        .base_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    let sender_name: Option<String> =
        sqlx::query_scalar("SELECT username FROM user_accounts WHERE user_id = $1")
            .bind(&sender_id)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten()
            .filter(|u: &String| !u.is_empty());
    let sender_qualified_id = format!("{}@{}", sender_name.as_deref().unwrap_or(&sender_id), host);

    let attachments_json = serde_json::to_string(&attachments).unwrap_or_else(|_| "[]".to_string());
    let msg_repo = MessageRepository::new(pool.clone());

    for member in members {
        let conn = match repo::get_connection_by_id(&pool, &member.server_connection_id).await {
            Ok(Some(c)) if c.status == "active" => c,
            Ok(Some(_)) => continue,
            Ok(None) => {
                warn!(
                    "forward_message_to_peers: connection {} not found",
                    member.server_connection_id
                );
                continue;
            }
            Err(e) => {
                error!("forward_message_to_peers: db error: {}", e);
                continue;
            }
        };

        let queue_id = Uuid::new_v4().to_string();

        // Persist queue entry (sets message.federated_status = 'pending').
        if let Err(e) = msg_repo
            .enqueue_federated(
                &queue_id,
                &message_id,
                &conn.id,
                &member.remote_user_id,
                &sender_qualified_id,
                &conversation_id,
                &conversation_type,
                group_name.as_deref(),
                &encrypted_content,
                &sent_at,
                &attachments_json,
            )
            .await
        {
            error!("forward_message_to_peers: enqueue failed: {}", e);
            continue;
        }

        // Spawn delivery, because each peer gets its own task/stack frame,
        // this might waste stack if futures are queued endlessly.
        tokio::spawn(deliver_queue_entry(
            pool.clone(),
            Arc::clone(&settings),
            Arc::clone(&identity),
            client.clone(),
            queue_id,
            conn.id,
            conn.address,
            member.remote_user_id,
            sender_qualified_id.clone(),
            conversation_id.clone(),
            conversation_type.clone(),
            group_name.clone(),
            encrypted_content.clone(),
            sent_at.clone(),
            attachments.clone(),
            enc.clone(),
        ));
    }
}

/// Attempt delivery of a single queued message to one peer.
/// On success marks the queue entry and (if all peers done) the message as delivered.
/// On failure leaves the queue entry pending for the reconnect drain.
#[allow(clippy::too_many_arguments)]
pub async fn deliver_queue_entry(
    pool: AnyPool,
    settings: Arc<Settings>,
    identity: Arc<ServerIdentity>,
    client: S2SClient,
    queue_id: String,
    conn_id: String,
    conn_address: String,
    recipient_user_id: String,
    sender_qualified_id: String,
    conversation_id: String,
    conversation_type: String,
    group_name: Option<String>,
    encrypted_content: String,
    sent_at: String,
    attachments: Vec<FederatedMediaRef>,
    enc: Option<ContentEncryption>,
) {
    let msg_repo = MessageRepository::new(pool.clone());

    // Load the shared secret fresh in case it was rotated.
    let conn = match repo::get_connection_by_id(&pool, &conn_id).await {
        Ok(Some(c)) => c,
        _ => {
            warn!("deliver_queue_entry: connection {} not found", conn_id);
            return;
        }
    };

    let raw_secret = match conn.shared_secret.as_deref() {
        Some(s) => s.to_vec(),
        None => {
            warn!("deliver_queue_entry: no shared secret for conn {}", conn_id);
            return;
        }
    };

    let shared_secret = match load_shared_secret(&raw_secret, enc.as_ref()) {
        Ok(s) => s,
        Err(e) => {
            error!("deliver_queue_entry: decrypt shared secret failed: {}", e);
            return;
        }
    };

    let inner = S2SInnerMessage {
        recipient_user_id,
        encrypted_content,
        sent_at: sent_at.clone(),
        attachments,
    };

    let inner_bytes = match serde_json::to_vec(&inner) {
        Ok(b) => b,
        Err(e) => {
            error!("deliver_queue_entry: serialize inner failed: {}", e);
            return;
        }
    };

    let (encrypted_payload, nonce) = match encrypt_s2s_payload(&shared_secret, &inner_bytes) {
        Ok(r) => r,
        Err(e) => {
            error!("deliver_queue_entry: encrypt failed: {}", e);
            return;
        }
    };

    let envelope = S2SMessageEnvelope {
        queue_id: queue_id.clone(),
        conversation_id,
        conversation_type,
        group_name,
        sender_qualified_id,
        encrypted_payload,
        nonce,
        sent_at,
    };

    let our_address = settings.base_url.trim_end_matches('/');
    let _ = msg_repo.record_attempt(&queue_id).await;

    match client::send_message(
        &client,
        &identity,
        our_address,
        &conn_address,
        envelope,
        &shared_secret,
    )
    .await
    {
        Ok(()) => {
            info!(
                "deliver_queue_entry: delivered {} to {}",
                queue_id, conn_address
            );
            if let Err(e) = msg_repo.mark_queue_delivered(&queue_id).await {
                error!("deliver_queue_entry: mark delivered failed: {}", e);
            }
        }
        Err(e) => {
            warn!(
                "deliver_queue_entry: send to {} failed (will retry on reconnect): {}",
                conn_address, e
            );
            // Queue entry stays 'pending' in case the receiving side is offline or unavailable,
            // drain_pending_for_peer picks it up on reconnect.
        }
    }
}

/// Called when a peer comes back online. Re-sends all pending queue entries for that peer.
/// Entries are delivered one at a time (sequentially) to avoid hammering the peer's SQLite
/// with concurrent writes, which causes "database is locked" errors on the receiver.
/// This function is always called via tokio::spawn so it runs on its own task/stack.
pub async fn drain_pending_for_peer(
    pool: AnyPool,
    settings: Arc<Settings>,
    identity: Arc<ServerIdentity>,
    client: S2SClient,
    conn_id: String,
    conn_address: String,
) {
    let msg_repo = MessageRepository::new(pool.clone());
    let entries = match msg_repo.list_pending_for_server(&conn_id).await {
        Ok(e) => e,
        Err(e) => {
            error!("drain_pending_for_peer({}): db error: {}", conn_address, e);
            return;
        }
    };

    if entries.is_empty() {
        return;
    }

    info!(
        "drain_pending_for_peer: {} queued messages to retry for {}",
        entries.len(),
        conn_address
    );

    let enc = settings
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| ContentEncryption::from_hex_key(key).ok());

    for entry in entries {
        let attachments: Vec<FederatedMediaRef> =
            serde_json::from_str(&entry.attachments_json).unwrap_or_default();

        // Await each delivery before starting the next, the receiver's SQLite can only
        // handle one writer at a time, so sequential delivery prevents lock contention.
        // This is not a "big brain design" but a "oopsie daisy" inspired decision.
        deliver_queue_entry(
            pool.clone(),
            Arc::clone(&settings),
            Arc::clone(&identity),
            client.clone(),
            entry.id,
            conn_id.clone(),
            conn_address.clone(),
            entry.recipient_user_id,
            entry.sender_qualified_id,
            entry.conversation_id,
            entry.conversation_type,
            entry.group_name,
            entry.encrypted_content,
            entry.sent_at,
            attachments,
            enc.clone(),
        )
        .await;
    }
}
