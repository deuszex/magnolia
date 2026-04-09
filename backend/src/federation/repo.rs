use chrono::Utc;
use sqlx::AnyPool;
use uuid::Uuid;

use super::models::{
    DiscoveryHintRow, FederatedPostContent, FederatedPostEntry, FederationPostRefRow,
    FederationUserRow, ServerConnectionRow, ServerIdentityRow, UserFederationSettingsRow,
};

// Server identity

pub async fn get_server_identity(pool: &AnyPool) -> sqlx::Result<Option<ServerIdentityRow>> {
    sqlx::query_as::<_, ServerIdentityRow>(
        "SELECT id, ml_dsa_public_key, ml_dsa_private_key, created_at
 FROM server_identity WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
}

pub async fn insert_server_identity(
    pool: &AnyPool,
    public_key: &[u8],
    private_key: &[u8],
) -> sqlx::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO server_identity (id, ml_dsa_public_key, ml_dsa_private_key, created_at)
 VALUES (1, $1, $2, $3)",
    )
    .bind(public_key)
    .bind(private_key)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

// Server connections

pub async fn list_connections(pool: &AnyPool) -> sqlx::Result<Vec<ServerConnectionRow>> {
    sqlx::query_as::<_, ServerConnectionRow>(
        "SELECT id, address, display_name, status,
 their_ml_dsa_public_key, their_ml_kem_encap_key, shared_secret,
 peer_is_shareable, peer_wants_discovery, we_share_connections,
 notes, local_ban_count, peer_request_id, created_at, connected_at, last_seen_at
 FROM server_connections
 ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
}

pub async fn get_connection_by_id(
    pool: &AnyPool,
    id: &str,
) -> sqlx::Result<Option<ServerConnectionRow>> {
    sqlx::query_as::<_, ServerConnectionRow>(
        "SELECT id, address, display_name, status,
 their_ml_dsa_public_key, their_ml_kem_encap_key, shared_secret,
 peer_is_shareable, peer_wants_discovery, we_share_connections,
 notes, local_ban_count, peer_request_id, created_at, connected_at, last_seen_at
 FROM server_connections WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn get_connection_by_address(
    pool: &AnyPool,
    address: &str,
) -> sqlx::Result<Option<ServerConnectionRow>> {
    sqlx::query_as::<_, ServerConnectionRow>(
        "SELECT id, address, display_name, status,
 their_ml_dsa_public_key, their_ml_kem_encap_key, shared_secret,
 peer_is_shareable, peer_wants_discovery, we_share_connections,
 notes, local_ban_count, peer_request_id, created_at, connected_at, last_seen_at
 FROM server_connections WHERE address = $1",
    )
    .bind(address)
    .fetch_optional(pool)
    .await
}

/// Returns all active connections — used when fanning out discovery or messages.
pub async fn list_active_connections(pool: &AnyPool) -> sqlx::Result<Vec<ServerConnectionRow>> {
    sqlx::query_as::<_, ServerConnectionRow>(
        "SELECT id, address, display_name, status,
 their_ml_dsa_public_key, their_ml_kem_encap_key, shared_secret,
 peer_is_shareable, peer_wants_discovery, we_share_connections,
 notes, local_ban_count, peer_request_id, created_at, connected_at, last_seen_at
 FROM server_connections WHERE status = 'active'",
    )
    .fetch_all(pool)
    .await
}

/// Returns shareable active connections (peer_is_shareable = 1).
pub async fn list_shareable_connections(pool: &AnyPool) -> sqlx::Result<Vec<ServerConnectionRow>> {
    sqlx::query_as::<_, ServerConnectionRow>(
        "SELECT id, address, display_name, status,
 their_ml_dsa_public_key, their_ml_kem_encap_key, shared_secret,
 peer_is_shareable, peer_wants_discovery, we_share_connections,
 notes, local_ban_count, peer_request_id, created_at, connected_at, last_seen_at
 FROM server_connections WHERE status = 'active' AND peer_is_shareable = 1",
    )
    .fetch_all(pool)
    .await
}

pub async fn insert_pending_out_connection(
    pool: &AnyPool,
    address: &str,
    their_ml_kem_encap_key: &[u8],
    we_share_connections: bool,
) -> sqlx::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO server_connections
 (id, address, status, their_ml_kem_encap_key, we_share_connections, created_at)
 VALUES ($1, $2, 'pending_out', $3, $4, $5)",
    )
    .bind(&id)
    .bind(address)
    .bind(their_ml_kem_encap_key)
    .bind(we_share_connections as i32)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn insert_pending_in_connection(
    pool: &AnyPool,
    address: &str,
    their_ml_dsa_public_key: &[u8],
    their_ml_kem_encap_key: &[u8],
    peer_is_shareable: bool,
    peer_wants_discovery: bool,
    peer_request_id: Option<&str>,
) -> sqlx::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO server_connections
 (id, address, status, their_ml_dsa_public_key, their_ml_kem_encap_key,
 peer_is_shareable, peer_wants_discovery, peer_request_id, created_at)
 VALUES ($1, $2, 'pending_in', $3, $4, $5, $6, $7, $8)",
    )
    .bind(&id)
    .bind(address)
    .bind(their_ml_dsa_public_key)
    .bind(their_ml_kem_encap_key)
    .bind(peer_is_shareable as i32)
    .bind(peer_wants_discovery as i32)
    .bind(peer_request_id)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn activate_connection(
    pool: &AnyPool,
    id: &str,
    their_ml_dsa_public_key: &[u8],
    shared_secret: &[u8],
    peer_is_shareable: bool,
    peer_wants_discovery: bool,
) -> sqlx::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE server_connections
 SET status = 'active',
 their_ml_dsa_public_key = $1,
 shared_secret = $2,
 peer_is_shareable = $3,
 peer_wants_discovery = $4,
 connected_at = $5,
 last_seen_at = $5
 WHERE id = $6",
    )
    .bind(their_ml_dsa_public_key)
    .bind(shared_secret)
    .bind(peer_is_shareable as i32)
    .bind(peer_wants_discovery as i32)
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Called when Side B accepts a pending_in request: stores the ciphertext-derived secret.
pub async fn activate_incoming_connection(
    pool: &AnyPool,
    id: &str,
    shared_secret: &[u8],
) -> sqlx::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE server_connections
 SET status = 'active', shared_secret = $1, connected_at = $2, last_seen_at = $2
 WHERE id = $3",
    )
    .bind(shared_secret)
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_connection_status(pool: &AnyPool, id: &str, status: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE server_connections SET status = $1 WHERE id = $2")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_connection_address(
    pool: &AnyPool,
    id: &str,
    address: &str,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE server_connections SET address = $1 WHERE id = $2")
        .bind(address)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_connection(pool: &AnyPool, id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM server_connections WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_connection_display_name(
    pool: &AnyPool,
    id: &str,
    display_name: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE server_connections SET display_name = $1 WHERE id = $2")
        .bind(display_name)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_connection_notes(
    pool: &AnyPool,
    id: &str,
    notes: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE server_connections SET notes = $1 WHERE id = $2")
        .bind(notes)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_last_seen(pool: &AnyPool, id: &str) -> sqlx::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE server_connections SET last_seen_at = $1 WHERE id = $2")
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn recalculate_ban_count(pool: &AnyPool, server_id: &str) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE server_connections
 SET local_ban_count = (
 SELECT COUNT(*) FROM user_external_bans WHERE server_connection_id = $1
 )
 WHERE id = $1",
    )
    .bind(server_id)
    .execute(pool)
    .await?;
    Ok(())
}

// Discovery hints

pub async fn list_discovery_hints(pool: &AnyPool) -> sqlx::Result<Vec<DiscoveryHintRow>> {
    sqlx::query_as::<_, DiscoveryHintRow>(
        "SELECT id, learned_from_server_id, address, status, created_at
 FROM discovery_hints WHERE status = 'suggested'
 ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
}

pub async fn upsert_discovery_hint(
    pool: &AnyPool,
    learned_from_server_id: &str,
    address: &str,
) -> sqlx::Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO discovery_hints (id, learned_from_server_id, address, status, created_at)
 VALUES ($1, $2, $3, 'suggested', $4)
 ON CONFLICT(learned_from_server_id, address) DO NOTHING",
    )
    .bind(&id)
    .bind(learned_from_server_id)
    .bind(address)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_discovery_hint_status(pool: &AnyPool, id: &str, status: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE discovery_hints SET status = $1 WHERE id = $2")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// Federation users

pub async fn list_federation_users_for_server(
    pool: &AnyPool,
    server_connection_id: &str,
) -> sqlx::Result<Vec<FederationUserRow>> {
    sqlx::query_as::<_, FederationUserRow>(
        "SELECT id, server_connection_id, remote_user_id, username,
 display_name, avatar_url, ecdh_public_key, last_synced_at
 FROM federation_users WHERE server_connection_id = $1",
    )
    .bind(server_connection_id)
    .fetch_all(pool)
    .await
}

pub async fn search_federation_users(
    pool: &AnyPool,
    query: &str,
    limit: i64,
    offset: i64,
) -> sqlx::Result<Vec<FederationUserRow>> {
    let pattern = format!("%{}%", query);
    sqlx::query_as::<_, FederationUserRow>(
        "SELECT fu.id, fu.server_connection_id, fu.remote_user_id, fu.username,
 fu.display_name, fu.avatar_url, fu.ecdh_public_key, fu.last_synced_at
 FROM federation_users fu
 JOIN server_connections sc ON sc.id = fu.server_connection_id
 WHERE sc.status = 'active'
 AND (fu.username LIKE $1 OR fu.display_name LIKE $1)
 LIMIT $2 OFFSET $3",
    )
    .bind(&pattern)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn search_federation_users_for_server(
    pool: &AnyPool,
    server_connection_id: &str,
    query: &str,
    limit: i64,
    offset: i64,
) -> sqlx::Result<Vec<FederationUserRow>> {
    let pattern = format!("%{}%", query);
    sqlx::query_as::<_, FederationUserRow>(
        "SELECT fu.id, fu.server_connection_id, fu.remote_user_id, fu.username,
 fu.display_name, fu.avatar_url, fu.ecdh_public_key, fu.last_synced_at
 FROM federation_users fu
 JOIN server_connections sc ON sc.id = fu.server_connection_id
 WHERE sc.status = 'active'
 AND fu.server_connection_id = $1
 AND (fu.username LIKE $2 OR fu.display_name LIKE $2)
 LIMIT $3 OFFSET $4",
    )
    .bind(server_connection_id)
    .bind(&pattern)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

/// Look up a federation user by their username (not UUID) for sender verification.
pub async fn get_federation_user_by_username(
    pool: &AnyPool,
    server_connection_id: &str,
    username: &str,
) -> sqlx::Result<Option<FederationUserRow>> {
    sqlx::query_as::<_, FederationUserRow>(
        "SELECT id, server_connection_id, remote_user_id, username,
 display_name, avatar_url, ecdh_public_key, last_synced_at
 FROM federation_users
 WHERE server_connection_id = $1 AND username = $2",
    )
    .bind(server_connection_id)
    .bind(username)
    .fetch_optional(pool)
    .await
}

pub async fn get_federation_user(
    pool: &AnyPool,
    server_connection_id: &str,
    remote_user_id: &str,
) -> sqlx::Result<Option<FederationUserRow>> {
    sqlx::query_as::<_, FederationUserRow>(
        "SELECT id, server_connection_id, remote_user_id, username,
 display_name, avatar_url, ecdh_public_key, last_synced_at
 FROM federation_users
 WHERE server_connection_id = $1 AND remote_user_id = $2",
    )
    .bind(server_connection_id)
    .bind(remote_user_id)
    .fetch_optional(pool)
    .await
}

pub async fn upsert_federation_user(
    pool: &AnyPool,
    server_connection_id: &str,
    remote_user_id: &str,
    username: &str,
    display_name: Option<&str>,
    avatar_url: Option<&str>,
    ecdh_public_key: Option<&str>,
) -> sqlx::Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO federation_users
 (id, server_connection_id, remote_user_id, username,
 display_name, avatar_url, ecdh_public_key, last_synced_at)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
 ON CONFLICT(server_connection_id, remote_user_id) DO UPDATE SET
 username = excluded.username,
 display_name = excluded.display_name,
 avatar_url = excluded.avatar_url,
 ecdh_public_key = excluded.ecdh_public_key,
 last_synced_at = excluded.last_synced_at",
    )
    .bind(&id)
    .bind(server_connection_id)
    .bind(remote_user_id)
    .bind(username)
    .bind(display_name)
    .bind(avatar_url)
    .bind(ecdh_public_key)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_federation_user(
    pool: &AnyPool,
    server_connection_id: &str,
    remote_user_id: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "DELETE FROM federation_users
 WHERE server_connection_id = $1 AND remote_user_id = $2",
    )
    .bind(server_connection_id)
    .bind(remote_user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_all_federation_users_for_server(
    pool: &AnyPool,
    server_connection_id: &str,
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM federation_users WHERE server_connection_id = $1")
        .bind(server_connection_id)
        .execute(pool)
        .await?;
    Ok(())
}

// Shareable user list

/// A local user who has opted in to federation sharing.
pub struct SharableUser {
    pub user_id: String,
    pub username: String,
    pub display_name: Option<String>,
    /// `avatar_media_id` from user_accounts — caller constructs the full URL.
    pub avatar_media_id: Option<String>,
    /// ECDH public key (stored as JWK by the e2e.js module via `public_key` column).
    pub ecdh_public_key: Option<String>,
}

/// Returns all active local users whose `sharing_mode` is not "off".
pub async fn list_shareable_users(pool: &AnyPool) -> sqlx::Result<Vec<SharableUser>> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT ua.user_id, ua.username, ua.display_name, ua.avatar_media_id, ua.public_key
 FROM user_accounts ua
 JOIN user_federation_settings ufs ON ua.user_id = ufs.user_id
 WHERE ufs.sharing_mode != 'off' AND ua.active = 1",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| SharableUser {
            user_id: r.get("user_id"),
            username: r.get::<Option<String>, _>("username").unwrap_or_default(),
            display_name: r.get("display_name"),
            avatar_media_id: r.get("avatar_media_id"),
            ecdh_public_key: r.get("public_key"),
        })
        .collect())
}

// User federation settings

pub async fn get_user_federation_settings(
    pool: &AnyPool,
    user_id: &str,
) -> sqlx::Result<Option<UserFederationSettingsRow>> {
    sqlx::query_as::<_, UserFederationSettingsRow>(
        "SELECT user_id, sharing_mode, post_sharing_mode
 FROM user_federation_settings WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub async fn upsert_user_federation_settings(
    pool: &AnyPool,
    user_id: &str,
    sharing_mode: &str,
    post_sharing_mode: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO user_federation_settings (user_id, sharing_mode, post_sharing_mode)
 VALUES ($1, $2, $3)
 ON CONFLICT(user_id) DO UPDATE SET
 sharing_mode = excluded.sharing_mode,
 post_sharing_mode = excluded.post_sharing_mode",
    )
    .bind(user_id)
    .bind(sharing_mode)
    .bind(post_sharing_mode)
    .execute(pool)
    .await?;
    Ok(())
}

/// Returns true if the local user should be shared with the given server.
pub async fn user_is_shareable_with(
    pool: &AnyPool,
    user_id: &str,
    server_connection_id: &str,
) -> sqlx::Result<bool> {
    let settings = match get_user_federation_settings(pool, user_id).await? {
        Some(s) => s,
        None => return Ok(false), // default: off
    };

    match settings.sharing_mode.as_str() {
        "off" => Ok(false),
        "whitelist" => {
            // Allowed only if an explicit allow rule exists.
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM user_federation_rules
 WHERE user_id = $1 AND server_connection_id = $2
 AND rule_type = 'sharing' AND effect = 'allow'",
            )
            .bind(user_id)
            .bind(server_connection_id)
            .fetch_one(pool)
            .await?;
            Ok(count > 0)
        }
        "blacklist" => {
            // Allowed unless an explicit deny rule exists.
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM user_federation_rules
 WHERE user_id = $1 AND server_connection_id = $2
 AND rule_type = 'sharing' AND effect = 'deny'",
            )
            .bind(user_id)
            .bind(server_connection_id)
            .fetch_one(pool)
            .await?;
            Ok(count == 0)
        }
        _ => Ok(false),
    }
}

// External bans

pub struct ExternalBanRow {
    pub server_connection_id: String,
    pub remote_user_id: String,
    pub banned_at: String,
}

pub async fn list_external_bans(
    pool: &AnyPool,
    local_user_id: &str,
) -> sqlx::Result<Vec<ExternalBanRow>> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT server_connection_id, remote_user_id, banned_at
 FROM user_external_bans
 WHERE local_user_id = $1
 ORDER BY banned_at DESC",
    )
    .bind(local_user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ExternalBanRow {
            server_connection_id: r.get("server_connection_id"),
            remote_user_id: r.get("remote_user_id"),
            banned_at: r.get("banned_at"),
        })
        .collect())
}

pub async fn insert_external_ban(
    pool: &AnyPool,
    local_user_id: &str,
    server_connection_id: &str,
    remote_user_id: &str,
) -> sqlx::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR IGNORE INTO user_external_bans
 (local_user_id, server_connection_id, remote_user_id, banned_at)
 VALUES ($1, $2, $3, $4)",
    )
    .bind(local_user_id)
    .bind(server_connection_id)
    .bind(remote_user_id)
    .bind(&now)
    .execute(pool)
    .await?;
    recalculate_ban_count(pool, server_connection_id).await?;
    Ok(())
}

pub async fn delete_external_ban(
    pool: &AnyPool,
    local_user_id: &str,
    server_connection_id: &str,
    remote_user_id: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "DELETE FROM user_external_bans
 WHERE local_user_id = $1 AND server_connection_id = $2 AND remote_user_id = $3",
    )
    .bind(local_user_id)
    .bind(server_connection_id)
    .bind(remote_user_id)
    .execute(pool)
    .await?;
    recalculate_ban_count(pool, server_connection_id).await?;
    Ok(())
}

pub async fn is_external_user_banned(
    pool: &AnyPool,
    local_user_id: &str,
    server_connection_id: &str,
    remote_user_id: &str,
) -> sqlx::Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_external_bans
 WHERE local_user_id = $1 AND server_connection_id = $2 AND remote_user_id = $3",
    )
    .bind(local_user_id)
    .bind(server_connection_id)
    .bind(remote_user_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

// Federation rules (per-server allow/deny)

pub struct FederationRuleRow {
    pub server_connection_id: String,
    pub rule_type: String,
    pub effect: String,
}

pub async fn list_federation_rules(
    pool: &AnyPool,
    user_id: &str,
) -> sqlx::Result<Vec<FederationRuleRow>> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT server_connection_id, rule_type, effect
 FROM user_federation_rules
 WHERE user_id = $1
 ORDER BY rule_type, server_connection_id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| FederationRuleRow {
            server_connection_id: r.get("server_connection_id"),
            rule_type: r.get("rule_type"),
            effect: r.get("effect"),
        })
        .collect())
}

pub async fn upsert_federation_rule(
    pool: &AnyPool,
    user_id: &str,
    server_connection_id: &str,
    rule_type: &str,
    effect: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO user_federation_rules
 (user_id, server_connection_id, rule_type, effect)
 VALUES ($1, $2, $3, $4)
 ON CONFLICT(user_id, server_connection_id, rule_type) DO UPDATE SET effect = $4",
    )
    .bind(user_id)
    .bind(server_connection_id)
    .bind(rule_type)
    .bind(effect)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_federation_rule(
    pool: &AnyPool,
    user_id: &str,
    server_connection_id: &str,
    rule_type: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "DELETE FROM user_federation_rules
 WHERE user_id = $1 AND server_connection_id = $2 AND rule_type = $3",
    )
    .bind(user_id)
    .bind(server_connection_id)
    .bind(rule_type)
    .execute(pool)
    .await?;
    Ok(())
}

pub struct EnrichedBanRow {
    pub server_connection_id: String,
    pub remote_user_id: String,
    pub banned_at: String,
    pub server_address: String,
    pub server_display_name: Option<String>,
    pub user_display_name: Option<String>,
    pub username: Option<String>,
}

pub async fn list_external_bans_enriched(
    pool: &AnyPool,
    local_user_id: &str,
) -> sqlx::Result<Vec<EnrichedBanRow>> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT b.server_connection_id, b.remote_user_id, b.banned_at,
 COALESCE(sc.address, '') AS server_address,
 sc.display_name AS server_display_name,
 fu.display_name AS user_display_name,
 fu.username
 FROM user_external_bans b
 LEFT JOIN server_connections sc ON sc.id = b.server_connection_id
 LEFT JOIN federation_users fu
 ON fu.server_connection_id = b.server_connection_id
 AND fu.remote_user_id = b.remote_user_id
 WHERE b.local_user_id = $1
 ORDER BY b.banned_at DESC",
    )
    .bind(local_user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| EnrichedBanRow {
            server_connection_id: r.get("server_connection_id"),
            remote_user_id: r.get("remote_user_id"),
            banned_at: r.get("banned_at"),
            server_address: r.get("server_address"),
            server_display_name: r.try_get("server_display_name").ok().flatten(),
            user_display_name: r.try_get("user_display_name").ok().flatten(),
            username: r.try_get("username").ok().flatten(),
        })
        .collect())
}

// Federated conversation members

pub struct FederatedMemberRow {
    pub id: String,
    pub conversation_id: String,
    pub server_connection_id: String,
    pub remote_user_id: String,
    pub remote_qualified_id: String,
    pub joined_at: String,
}

/// Find an existing federated DM conversation for a (local_user, remote_user@server) pair.
pub async fn find_federated_dm(
    pool: &AnyPool,
    local_user_id: &str,
    server_connection_id: &str,
    remote_user_id: &str,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar::<_, String>(
        "SELECT fcm.conversation_id
 FROM federated_conversation_members fcm
 JOIN conversation_members cm ON cm.conversation_id = fcm.conversation_id
 JOIN conversations c ON c.conversation_id = fcm.conversation_id
 WHERE fcm.server_connection_id = $1
 AND fcm.remote_user_id = $2
 AND cm.user_id = $3
 AND c.conversation_type = 'direct'
 LIMIT 1",
    )
    .bind(server_connection_id)
    .bind(remote_user_id)
    .bind(local_user_id)
    .fetch_optional(pool)
    .await
}

/// Create a new federated DM conversation. Inserts the conversation,
/// a local member row, and a federated member row.
pub async fn create_federated_dm(
    pool: &AnyPool,
    local_user_id: &str,
    server_connection_id: &str,
    remote_user_id: &str,
    remote_qualified_id: &str,
) -> sqlx::Result<String> {
    let conv_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO conversations
 (conversation_id, conversation_type, created_by, created_at, updated_at)
 VALUES ($1, 'direct', '__fed__', $2, $2)",
    )
    .bind(&conv_id)
    .bind(&now)
    .execute(pool)
    .await?;

    let member_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO conversation_members (id, conversation_id, user_id, role, joined_at)
 VALUES ($1, $2, $3, 'member', $4)",
    )
    .bind(&member_id)
    .bind(&conv_id)
    .bind(local_user_id)
    .bind(&now)
    .execute(pool)
    .await?;

    add_federated_member(
        pool,
        &conv_id,
        server_connection_id,
        remote_user_id,
        remote_qualified_id,
    )
    .await?;

    Ok(conv_id)
}

/// Add a remote user as a federated participant in a conversation.
pub async fn add_federated_member(
    pool: &AnyPool,
    conversation_id: &str,
    server_connection_id: &str,
    remote_user_id: &str,
    remote_qualified_id: &str,
) -> sqlx::Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR IGNORE INTO federated_conversation_members
 (id, conversation_id, server_connection_id,
 remote_user_id, remote_qualified_id, joined_at)
 VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(&id)
    .bind(conversation_id)
    .bind(server_connection_id)
    .bind(remote_user_id)
    .bind(remote_qualified_id)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

/// List all federated members of a conversation.
pub async fn list_federated_members(
    pool: &AnyPool,
    conversation_id: &str,
) -> sqlx::Result<Vec<FederatedMemberRow>> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, conversation_id, server_connection_id,
 remote_user_id, remote_qualified_id, joined_at
 FROM federated_conversation_members
 WHERE conversation_id = $1",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| FederatedMemberRow {
            id: r.get("id"),
            conversation_id: r.get("conversation_id"),
            server_connection_id: r.get("server_connection_id"),
            remote_user_id: r.get("remote_user_id"),
            remote_qualified_id: r.get("remote_qualified_id"),
            joined_at: r.get("joined_at"),
        })
        .collect())
}

/// List local user_ids who are members of a conversation.
pub async fn list_local_members_of_conversation(
    pool: &AnyPool,
    conversation_id: &str,
) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar::<_, String>(
        "SELECT user_id FROM conversation_members WHERE conversation_id = $1",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await
}

// Federation post refs

pub async fn upsert_post_ref(
    pool: &AnyPool,
    server_connection_id: &str,
    remote_user_id: &str,
    remote_post_id: &str,
    posted_at: &str,
    content_hash: &str,
    content_json: &str,
    server_address: &str,
) -> sqlx::Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO federation_post_refs
 (id, server_connection_id, remote_user_id, remote_post_id,
 posted_at, content_hash, first_seen_at, content_json, server_address)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
 ON CONFLICT(server_connection_id, remote_user_id, remote_post_id) DO UPDATE SET
 content_hash = excluded.content_hash,
 posted_at = excluded.posted_at,
 content_json = excluded.content_json,
 server_address = excluded.server_address",
    )
    .bind(&id)
    .bind(server_connection_id)
    .bind(remote_user_id)
    .bind(remote_post_id)
    .bind(posted_at)
    .bind(content_hash)
    .bind(&now)
    .bind(content_json)
    .bind(server_address)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_post_ref(
    pool: &AnyPool,
    server_connection_id: &str,
    remote_post_id: &str,
) -> sqlx::Result<Option<FederationPostRefRow>> {
    sqlx::query_as::<_, FederationPostRefRow>(
        "SELECT id, server_connection_id, remote_user_id, remote_post_id,
 posted_at, content_hash, first_seen_at, content_json, server_address
 FROM federation_post_refs
 WHERE server_connection_id = $1 AND remote_post_id = $2",
    )
    .bind(server_connection_id)
    .bind(remote_post_id)
    .fetch_optional(pool)
    .await
}

/// List all cached post refs (with content) for the federated feed, newest first.
pub async fn list_all_post_refs(
    pool: &AnyPool,
    limit: i64,
    offset: i64,
) -> sqlx::Result<Vec<FederationPostRefRow>> {
    sqlx::query_as::<_, FederationPostRefRow>(
        "SELECT id, server_connection_id, remote_user_id, remote_post_id,
 posted_at, content_hash, first_seen_at, content_json, server_address
 FROM federation_post_refs
 WHERE content_json IS NOT NULL
 ORDER BY posted_at DESC
 LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

/// Return posts from locally-opted-in users that are shareable with `server_connection_id`.
/// Constructs absolute media URLs using `our_address`.
pub async fn list_posts_for_peer(
    pool: &AnyPool,
    server_connection_id: &str,
    our_address: &str,
    limit: i64,
    before: Option<&str>,
) -> sqlx::Result<Vec<FederatedPostEntry>> {
    use sqlx::Row;

    let base = our_address.trim_end_matches('/');

    // Fetch post list, respecting post_sharing_mode whitelist/blacklist.
    let post_rows: Vec<(String, String, String)> = if let Some(before_ts) = before {
        sqlx::query(
            "SELECT p.post_id, p.author_id, p.created_at
 FROM posts p
 JOIN user_accounts ua ON ua.user_id = p.author_id
 JOIN user_federation_settings ufs ON ufs.user_id = p.author_id
 WHERE p.is_published = 1 AND ua.active = 1
 AND ufs.post_sharing_mode != 'off'
 AND (ufs.post_sharing_mode != 'whitelist' OR EXISTS (
 SELECT 1 FROM user_federation_rules ufr
 WHERE ufr.user_id = p.author_id
 AND ufr.server_connection_id = $1
 AND ufr.rule_type = 'posts' AND ufr.effect = 'allow'))
 AND (ufs.post_sharing_mode != 'blacklist' OR NOT EXISTS (
 SELECT 1 FROM user_federation_rules ufr
 WHERE ufr.user_id = p.author_id
 AND ufr.server_connection_id = $1
 AND ufr.rule_type = 'posts' AND ufr.effect = 'deny'))
 AND p.created_at < $2
 ORDER BY p.created_at DESC LIMIT $3",
        )
        .bind(server_connection_id)
        .bind(before_ts)
        .bind(limit)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| (r.get(0), r.get(1), r.get(2)))
        .collect()
    } else {
        sqlx::query(
            "SELECT p.post_id, p.author_id, p.created_at
 FROM posts p
 JOIN user_accounts ua ON ua.user_id = p.author_id
 JOIN user_federation_settings ufs ON ufs.user_id = p.author_id
 WHERE p.is_published = 1 AND ua.active = 1
 AND ufs.post_sharing_mode != 'off'
 AND (ufs.post_sharing_mode != 'whitelist' OR EXISTS (
 SELECT 1 FROM user_federation_rules ufr
 WHERE ufr.user_id = p.author_id
 AND ufr.server_connection_id = $1
 AND ufr.rule_type = 'posts' AND ufr.effect = 'allow'))
 AND (ufs.post_sharing_mode != 'blacklist' OR NOT EXISTS (
 SELECT 1 FROM user_federation_rules ufr
 WHERE ufr.user_id = p.author_id
 AND ufr.server_connection_id = $1
 AND ufr.rule_type = 'posts' AND ufr.effect = 'deny'))
 ORDER BY p.created_at DESC LIMIT $2",
        )
        .bind(server_connection_id)
        .bind(limit)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| (r.get(0), r.get(1), r.get(2)))
        .collect()
    };

    let mut entries = Vec::with_capacity(post_rows.len());

    for (post_id, author_id, posted_at) in post_rows {
        // Author info
        let author_row = sqlx::query(
            "SELECT username, display_name, avatar_media_id FROM user_accounts WHERE user_id = $1",
        )
        .bind(&author_id)
        .fetch_optional(pool)
        .await?;

        let (author_name, author_avatar_url) = match author_row {
            Some(r) => {
                let uname: Option<String> = r.get("username");
                let dname: Option<String> = r.get("display_name");
                let avatar_id: Option<String> = r.get("avatar_media_id");
                let display = dname.or(uname);
                let avatar = avatar_id.map(|id| format!("{}/api/media/{}/thumbnail", base, id));
                (display, avatar)
            }
            None => (None, None),
        };

        // Post contents — LEFT JOIN media to get width/height for media refs.
        let content_rows = sqlx::query(
            "SELECT pc.content_type, pc.display_order, pc.content, pc.original_filename,
                    pc.mime_type, pc.file_size, m.width, m.height
             FROM post_contents pc
             LEFT JOIN media m ON pc.content_type != 'text' AND m.media_id = pc.content
             WHERE pc.post_id = $1 ORDER BY pc.display_order",
        )
        .bind(&post_id)
        .fetch_all(pool)
        .await?;

        let contents: Vec<FederatedPostContent> = content_rows
            .into_iter()
            .map(|r| {
                let ct: String = r.get("content_type");
                let raw_content: String = r.get("content");
                let mime_type: Option<String> = r.get("mime_type");
                let file_size: Option<i64> = r.get("file_size");
                let filename: Option<String> = r.get("original_filename");
                let (content, media_ref) = if ct == "text" {
                    (raw_content, None)
                } else {
                    let mr = super::models::FederatedMediaRef {
                        media_id: raw_content,
                        media_type: ct.clone(),
                        mime_type: mime_type.clone().unwrap_or_default(),
                        file_size: file_size.unwrap_or(0),
                        filename: filename.clone().unwrap_or_default(),
                        width: r.get("width"),
                        height: r.get("height"),
                    };
                    (String::new(), Some(mr))
                };
                FederatedPostContent {
                    content_type: ct,
                    display_order: r.get("display_order"),
                    content,
                    media_ref,
                    mime_type,
                    file_size,
                    filename,
                }
            })
            .collect();

        // Tags
        let tag_rows = sqlx::query("SELECT tag FROM post_tags WHERE post_id = $1")
            .bind(&post_id)
            .fetch_all(pool)
            .await?;
        let tags: Vec<String> = tag_rows.into_iter().map(|r| r.get(0)).collect();

        // Content hash (SHA-256 of serialised contents)
        let content_hash = {
            use sha2::{Digest, Sha256};
            let json = serde_json::to_string(&contents).unwrap_or_default();
            hex::encode(Sha256::digest(json.as_bytes()))
        };

        entries.push(FederatedPostEntry {
            post_id,
            user_id: author_id,
            posted_at,
            content_hash,
            contents,
            author_name,
            author_avatar_url,
            tags,
            server_address: base.to_string(),
        });
    }

    Ok(entries)
}

/// Return a display name for a federated DM conversation, using the remote user's
/// display_name from federation_users (falls back to remote_qualified_id).
pub async fn get_federated_dm_display_name(
    pool: &AnyPool,
    conversation_id: &str,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar::<_, String>(
 "SELECT COALESCE(NULLIF(fu.display_name, ''), NULLIF(fu.username, ''), fcm.remote_qualified_id)
 FROM federated_conversation_members fcm
 LEFT JOIN federation_users fu
 ON fu.server_connection_id = fcm.server_connection_id
 AND fu.remote_user_id = fcm.remote_user_id
 WHERE fcm.conversation_id = $1
 LIMIT 1",
 )
 .bind(conversation_id)
 .fetch_optional(pool)
 .await
}
