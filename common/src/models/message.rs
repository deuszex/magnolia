use serde::{Deserialize, Serialize};

/// An E2E-encrypted message (server stores opaque ciphertext)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Message {
    pub message_id: String,
    pub conversation_id: String,
    /// `"__fed__"` for messages received via S2S federation; local user_id otherwise.
    pub sender_id: String,
    /// Set for federated messages: `"username@server"`. `None` for local messages.
    pub remote_sender_qualified_id: Option<String>,
    /// Base64-encoded ciphertext from client (or inner S2S payload for federated messages)
    pub encrypted_content: String,
    pub created_at: String,
    /// Outbound federation delivery state. NULL = local-only or inbound federated message.
    /// "pending" = not yet acknowledged by remote server. "delivered" = remote ACKed.
    pub federated_status: Option<String>,
}

/// Per-recipient delivery tracking for a message
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MessageDelivery {
    pub id: String,
    pub message_id: String,
    pub recipient_id: String,
    pub delivered_at: Option<String>,
}
