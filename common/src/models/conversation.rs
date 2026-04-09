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
}
