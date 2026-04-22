use serde::{Deserialize, Serialize};

/// Sentinel value for `sender_id` when a message was sent by a proxy account.
pub const PROXY_SENDER_SENTINEL: &str = "__proxy__";

/// An E2E-encrypted message (server stores opaque ciphertext)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Message {
    pub message_id: String,
    pub conversation_id: String,
    /// `"__fed__"` for federated messages; `"__proxy__"` for proxy-sent messages;
    /// local user_id otherwise.
    pub sender_id: String,
    /// Set for federated messages: `"username@server"`. `None` for local messages.
    pub remote_sender_qualified_id: Option<String>,
    /// Set when `sender_id == "__proxy__"`. References `proxy_accounts.proxy_id`.
    pub proxy_sender_id: Option<String>,
    /// Base64-encoded ciphertext from client (or inner S2S payload for federated messages)
    pub encrypted_content: String,
    pub created_at: String,
    /// Outbound federation delivery state. NULL = local-only or inbound federated message.
    /// "pending" = not yet acknowledged by remote server. "delivered" = remote ACKed.
    pub federated_status: Option<String>,
    /// Hex-encoded AES-256-GCM nonce when content is encrypted at rest; None = plaintext.
    pub content_nonce: Option<String>,
}

/// Per-recipient delivery tracking for a message
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MessageDelivery {
    pub id: String,
    pub message_id: String,
    pub recipient_id: String,
    pub delivered_at: Option<String>,
}
