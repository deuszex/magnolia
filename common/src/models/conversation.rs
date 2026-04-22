use serde::{Deserialize, Serialize};

/// A conversation (DM or group chat)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Conversation {
    pub conversation_id: String,
    /// "direct" or "group"
    pub conversation_type: String,
    /// Group name (NULL for DMs)
    pub name: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    /// Set when a proxy account created the conversation; `created_by` holds `__proxy__` sentinel.
    pub proxy_creator_id: Option<String>,
}

/// A member of a conversation
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ConversationMember {
    pub id: String,
    pub conversation_id: String,
    pub user_id: String,
    /// "owner", "admin", or "member"
    pub role: String,
    pub joined_at: String,
    /// Set when a proxy account is the member; `user_id` holds `__proxy__` sentinel.
    pub proxy_user_id: Option<String>,
}
