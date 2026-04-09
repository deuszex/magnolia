use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// DB row types

#[derive(Debug, Clone, FromRow)]
pub struct ServerIdentityRow {
    pub id: i64,
    pub ml_dsa_public_key: Vec<u8>,
    pub ml_dsa_private_key: Vec<u8>,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct ServerConnectionRow {
    pub id: String,
    pub address: String,
    pub display_name: Option<String>,
    pub status: String,
    pub their_ml_dsa_public_key: Option<Vec<u8>>,
    pub their_ml_kem_encap_key: Option<Vec<u8>>,
    pub shared_secret: Option<Vec<u8>>,
    pub peer_is_shareable: i32,
    pub peer_wants_discovery: i32,
    pub we_share_connections: i32,
    pub notes: Option<String>,
    pub local_ban_count: i32,
    /// Stored on Side B: the connection ID that Side A created for this pending_out record.
    /// Echoed back in ConnectionAccept so Side A can look up by ID instead of address.
    pub peer_request_id: Option<String>,
    pub created_at: String,
    pub connected_at: Option<String>,
    pub last_seen_at: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct DiscoveryHintRow {
    pub id: String,
    pub learned_from_server_id: String,
    pub address: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct FederationUserRow {
    pub id: String,
    pub server_connection_id: String,
    pub remote_user_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub ecdh_public_key: Option<String>,
    pub last_synced_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct UserFederationSettingsRow {
    pub user_id: String,
    pub sharing_mode: String,
    pub post_sharing_mode: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct FederationPostRefRow {
    pub id: String,
    pub server_connection_id: String,
    pub remote_user_id: String,
    pub remote_post_id: String,
    pub posted_at: String,
    pub content_hash: String,
    pub first_seen_at: String,
    /// Cached serialised `FederatedPostEntry` JSON (from migration 005).
    pub content_json: Option<String>,
    /// Base URL of the originating server (from migration 005).
    pub server_address: Option<String>,
}

// Status enums

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    PendingOut,
    PendingIn,
    Active,
    Rejected,
    Revoked,
}

impl ConnectionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingOut => "pending_out",
            Self::PendingIn => "pending_in",
            Self::Active => "active",
            Self::Rejected => "rejected",
            Self::Revoked => "revoked",
        }
    }
}

impl std::fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharingMode {
    Off,
    Whitelist,
    Blacklist,
}

impl SharingMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Whitelist => "whitelist",
            Self::Blacklist => "blacklist",
        }
    }
}

// API response types (returned to clients / admin)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConnectionDto {
    pub id: String,
    pub address: String,
    pub display_name: Option<String>,
    pub status: String,
    pub peer_is_shareable: bool,
    pub we_share_connections: bool,
    pub local_ban_count: i32,
    pub notes: Option<String>,
    pub created_at: String,
    pub connected_at: Option<String>,
    pub last_seen_at: Option<String>,
}

impl From<ServerConnectionRow> for ServerConnectionDto {
    fn from(r: ServerConnectionRow) -> Self {
        Self {
            id: r.id,
            address: r.address,
            display_name: r.display_name,
            status: r.status,
            peer_is_shareable: r.peer_is_shareable != 0,
            we_share_connections: r.we_share_connections != 0,
            local_ban_count: r.local_ban_count,
            notes: r.notes,
            created_at: r.created_at,
            connected_at: r.connected_at,
            last_seen_at: r.last_seen_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryHintDto {
    pub id: String,
    pub learned_from_server_id: String,
    pub address: String,
    pub status: String,
    pub created_at: String,
}

impl From<DiscoveryHintRow> for DiscoveryHintDto {
    fn from(r: DiscoveryHintRow) -> Self {
        Self {
            id: r.id,
            learned_from_server_id: r.learned_from_server_id,
            address: r.address,
            status: r.status,
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationUserDto {
    pub id: String,
    pub server_connection_id: String,
    pub remote_user_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    /// Qualified display: "username@server_address"
    pub qualified_name: String,
    pub last_synced_at: String,
}

impl FederationUserDto {
    pub fn from_row_and_address(row: FederationUserRow, server_address: &str) -> Self {
        // Strip scheme from address for @-notation: https://example.com → example.com
        let host = server_address
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/');
        let qualified_name = format!("{}@{}", row.username, host);
        Self {
            id: row.id,
            server_connection_id: row.server_connection_id,
            remote_user_id: row.remote_user_id,
            username: row.username,
            display_name: row.display_name,
            avatar_url: row.avatar_url,
            qualified_name,
            last_synced_at: row.last_synced_at,
        }
    }
}

// S2S wire types (exchanged between servers)

/// Sent by Side A when initiating a connection request.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionRequest {
    /// The requesting server's BASE_URL.
    pub address: String,
    /// ML-DSA-87 verifying key (hex).
    pub ml_dsa_public_key: String,
    /// ML-KEM-1024 encapsulation key (hex) — Side B uses this to produce the shared secret.
    pub ml_kem_encap_key: String,
    /// Whether Side A allows itself to be mentioned in discovery relays.
    pub is_shareable: bool,
    /// Whether Side A wants to receive Side B's known-connection list.
    pub wants_discovery: bool,
    /// Side A's local connection ID for this pending_out record.
    /// Side B stores this and echoes it back in ConnectionAccept so Side A can look up
    /// the record by ID rather than by address (avoids base_url mismatch issues).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// Sent by Side B in response to accepting a connection request.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionAccept {
    /// Side B's BASE_URL.
    pub address: String,
    /// Side B's ML-DSA-87 verifying key (hex).
    pub ml_dsa_public_key: String,
    /// ML-KEM-1024 ciphertext (hex) — Side A decapsulates to recover shared secret.
    pub ml_kem_ciphertext: String,
    /// Whether Side B allows itself to be mentioned in discovery relays.
    pub is_shareable: bool,
    /// Whether Side B wants to receive Side A's known-connection list.
    pub wants_discovery: bool,
    /// Echoed back from ConnectionRequest.request_id (Side A's connection ID).
    /// Lets Side A look up its pending_out record by ID instead of address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// Sent by Side B to reject a pending connection request.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionReject {
    pub address: String,
    pub reason: Option<String>,
}

/// Entry in a user-list sync push.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedUserEntry {
    pub remote_user_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    /// ECDH public key (base64) for user-level E2E encryption.
    pub ecdh_public_key: Option<String>,
}

/// Pushed by a server to sync its shareable user list with a peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserListSync {
    /// Full list replaces the peer's cached list for this server.
    pub users: Vec<FederatedUserEntry>,
}

/// Decrypted inner payload of a cross-server message.
///
/// After the outer AES-256-GCM (ML-KEM shared secret) layer is removed,
/// the bytes are parsed as this struct.
#[derive(Debug, Serialize, Deserialize)]
pub struct S2SInnerMessage {
    /// Local `user_id` of the intended recipient on the receiving server.
    /// Used to route new DMs to the correct local user.
    pub recipient_user_id: String,
    /// E2E-encrypted content blob — opaque to the server, decrypted client-side.
    pub encrypted_content: String,
    pub sent_at: String,
    /// Media attachments. Empty for text-only messages.
    /// Serde default so old peers that omit this field still deserialise cleanly.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<FederatedMediaRef>,
}

/// A single cross-server message delivery.
#[derive(Debug, Serialize, Deserialize)]
pub struct S2SMessageEnvelope {
    /// Stable delivery-queue ID from the sender's cross_server_message_queue.
    /// Echoed back in the 200 OK so the sender can mark the entry delivered.
    /// Also used by the receiver for idempotency (INSERT OR IGNORE keyed on this).
    pub queue_id: String,
    /// The sender's local conversation_id. For group messages this acts as the
    /// canonical group ID: the receiver maps it to their own local conversation
    /// via the remote_group_id column.
    pub conversation_id: String,
    /// "direct" or "group". Defaults to "direct" for backward-compat with old peers.
    #[serde(default = "default_conv_type")]
    pub conversation_type: String,
    /// Group display name — only set for group messages, used when creating the
    /// local conversation on the receiving server for the first time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    /// Sender: "remote_user_id@sender_server_address"
    pub sender_qualified_id: String,
    /// AES-256-GCM encrypted payload (base64), keyed with the shared ML-KEM secret.
    /// The inner content is itself ECDH-encrypted for the recipient(s).
    pub encrypted_payload: String,
    /// Nonce used for the AES-256-GCM outer layer (hex).
    pub nonce: String,
    pub sent_at: String,
}

fn default_conv_type() -> String {
    "direct".to_string()
}

/// Response to GET /api/s2s/users/identity/{username} — provides the
/// canonical profile of a single user for sender-identity verification.
#[derive(Debug, Serialize, Deserialize)]
pub struct UserIdentityResponse {
    pub remote_user_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub ecdh_public_key: Option<String>,
}

/// Sent when a remote user's ECDH public key is requested.
#[derive(Debug, Serialize, Deserialize)]
pub struct EcdhKeyResponse {
    pub remote_user_id: String,
    /// ECDH public key (base64).
    pub ecdh_public_key: String,
    /// ML-DSA signature over (remote_user_id || ecdh_public_key) confirming authenticity.
    pub signature: String,
}

/// A discovery hint pushed from one server to another.
#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoveryPush {
    pub known_servers: Vec<String>, // list of BASE_URLs
}

/// Shared header values extracted from an inbound S2S request.
#[derive(Debug, Clone)]
pub struct S2SRequestMeta {
    pub sender_address: String,
    pub timestamp: i64,
    pub nonce: String,
    pub signature: Vec<u8>,
}

// S2S post sharing

/// Compact media descriptor sent between servers — no URLs, just IDs and metadata.
/// The receiving server creates a local stub row and fetches the bytes lazily.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedMediaRef {
    /// media_id on the origin server
    pub media_id: String,
    /// "image", "video", "file"
    pub media_type: String,
    pub mime_type: String,
    pub file_size: i64,
    pub filename: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

/// A single content block inside a federated post.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedPostContent {
    /// "text", "image", "video", "file"
    pub content_type: String,
    pub display_order: i32,
    /// Text content for type=text blocks. Empty string for media blocks.
    pub content: String,
    /// Present for image/video/file blocks. Replaces the old absolute-URL approach.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_ref: Option<FederatedMediaRef>,
    pub mime_type: Option<String>,
    pub file_size: Option<i64>,
    pub filename: Option<String>,
}

/// A single post shared by a remote server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedPostEntry {
    pub post_id: String,
    pub user_id: String,
    pub posted_at: String,
    /// SHA-256 of the serialised `contents` — detects edits on re-fetch.
    pub content_hash: String,
    pub contents: Vec<FederatedPostContent>,
    pub author_name: Option<String>,
    pub author_avatar_url: Option<String>,
    pub tags: Vec<String>,
    /// Originating server base URL — set by the sender, used for media URL resolution.
    pub server_address: String,
}

/// Body for `POST /api/s2s/posts`.
#[derive(Debug, Serialize, Deserialize)]
pub struct S2SPostsRequest {
    #[serde(default = "default_posts_limit")]
    pub limit: i64,
    pub before: Option<String>,
}

fn default_posts_limit() -> i64 {
    20
}

/// Response body for `POST /api/s2s/posts`.
#[derive(Debug, Serialize, Deserialize)]
pub struct FederatedPostListResponse {
    pub posts: Vec<FederatedPostEntry>,
    pub has_more: bool,
}
