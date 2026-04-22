use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::schemas::post::CreatePostContentRequest;

/// Response representing a proxy account (safe to send to managing user/admin)
#[derive(Debug, Serialize)]
pub struct ProxyUserResponse {
    pub proxy_id: String,
    pub paired_user_id: Option<String>,
    pub active: bool,
    pub display_name: Option<String>,
    pub username: String,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub public_key: Option<String>,
    pub has_password: bool,
    pub has_e2e_key: bool,
    /// Whether a HMAC key is configured
    pub has_hmac_key: bool,
    /// First 8 hex chars of the HMAC key for fingerprint display (never the full key)
    pub hmac_key_fingerprint: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Request to set the proxy's HMAC signing key
#[derive(Debug, Deserialize)]
pub struct SetProxyHmacKeyRequest {
    /// The full hex-encoded HMAC key (64 chars = 32 bytes)
    pub hmac_key: String,
}

/// Admin request to create a new proxy account
#[derive(Debug, Deserialize, Validate)]
pub struct CreateProxyRequest {
    #[validate(length(min = 2, max = 30))]
    pub username: String,
    /// Optional user to pair this proxy to
    pub paired_user_id: Option<String>,
}

/// Request to update a proxy's profile info
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateProxyRequest {
    #[validate(length(max = 100))]
    pub display_name: Option<String>,
    #[validate(length(max = 500))]
    pub bio: Option<String>,
    pub avatar_media_id: Option<String>,
    /// Set to null string to remove
    pub active: Option<bool>,
}

/// Request to set/change proxy session password
#[derive(Debug, Deserialize, Validate)]
pub struct SetProxyPasswordRequest {
    #[validate(length(min = 8, max = 128))]
    pub password: String,
}

/// Request to set proxy E2E key blob
#[derive(Debug, Deserialize)]
pub struct SetProxyE2EKeyRequest {
    pub e2e_key_blob: String,
    pub public_key: String,
}

/// Login request for proxy session
#[derive(Debug, Deserialize)]
pub struct ProxyLoginRequest {
    pub username: String,
    pub password: String,
}

/// Response after successful proxy login
#[derive(Debug, Serialize)]
pub struct ProxyAuthResponse {
    pub proxy_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// HMAC send-message request (proxy one-shot)
#[derive(Debug, Deserialize, Validate)]
pub struct ProxyHmacSendMessageRequest {
    pub proxy_id: String,
    pub conversation_id: String,
    /// Opaque ciphertext; required (matches `messages.encrypted_content NOT NULL`)
    pub encrypted_content: String,
    /// Optional media attachments already uploaded to /api/media
    pub media_ids: Option<Vec<String>>,
    /// HMAC-SHA256 hex signature over:
    /// `proxy_id + ":" + conversation_id + ":" + sha256(encrypted_content) + ":" + timestamp`
    pub signature: String,
    /// Unix timestamp (seconds); must be within 5 minutes of server time
    pub timestamp: i64,
}

/// Per-proxy rate limit override (null = use server default)
#[derive(Debug, Serialize, Deserialize)]
pub struct ProxyRateLimitResponse {
    /// Max media uploads per minute (null = use server default)
    pub max_pieces_per_minute: Option<i32>,
    /// Max upload bytes per minute (null = use server default)
    pub max_bytes_per_minute: Option<i64>,
}

/// Request to set a per-proxy rate limit override
#[derive(Debug, Deserialize)]
pub struct SetProxyRateLimitRequest {
    /// Set to null to remove the override and fall back to the server default
    pub max_pieces_per_minute: Option<i32>,
    /// Set to null to remove the override and fall back to the server default
    pub max_bytes_per_minute: Option<i64>,
}

/// HMAC get-or-create-conversation request
#[derive(Debug, Deserialize, Validate)]
pub struct ProxyHmacGetOrCreateConversationRequest {
    pub proxy_id: String,
    /// Target user by ID. Mutually exclusive with target_username.
    pub target_user_id: Option<String>,
    /// Target user by username. Mutually exclusive with target_user_id.
    pub target_username: Option<String>,
    /// HMAC-SHA256 hex signature over `proxy_id + ":" + timestamp`
    pub signature: String,
    pub timestamp: i64,
}

/// Response for HMAC get-or-create-conversation
#[derive(Debug, Serialize)]
pub struct ProxyHmacConversationResponse {
    pub conversation_id: String,
    /// true if the conversation was just created, false if it already existed
    pub created: bool,
}

/// HMAC upload-media response
#[derive(Debug, Serialize)]
pub struct ProxyHmacMediaUploadResponse {
    pub media_id: String,
    pub url: String,
    pub thumbnail_url: Option<String>,
}

/// HMAC create-post request (proxy one-shot)
#[derive(Debug, Deserialize, Validate)]
pub struct ProxyHmacCreatePostRequest {
    pub proxy_id: String,
    /// Content items for the post (1-20), matching CreatePostRequest
    #[validate(length(min = 1, max = 20, message = "Post must have 1-20 content items"))]
    pub contents: Vec<CreatePostContentRequest>,
    /// Whether to publish immediately (default: false = draft)
    #[serde(default)]
    pub publish: bool,
    /// Tags for the post
    #[serde(default)]
    pub tags: Vec<String>,
    /// HMAC-SHA256 hex signature over:
    /// `proxy_id + ":" + sha256(canonical_body) + ":" + publish_bit + ":" + timestamp`
    ///
    /// canonical_body = contents sorted by display_order, each as "order|type|content",
    /// joined by "\n", then "\ntags:sorted_csv" and "\npublish:0|1"
    pub signature: String,
    /// Unix timestamp (seconds); must be within 5 minutes of server time
    pub timestamp: i64,
}
