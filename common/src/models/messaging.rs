use serde::{Deserialize, Serialize};

/// Per-user messaging preferences
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MessagingPreferences {
    pub user_id: String,
    /// 1 = accept DMs from anyone, 0 = reject all
    pub accept_messages: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// A user block entry (blocker -> blocked)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserBlock {
    pub id: String,
    pub user_id: String,
    pub blocked_user_id: String,
    pub created_at: String,
}
