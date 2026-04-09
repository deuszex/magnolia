use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ConversationFavourite {
    pub user_id: String,
    pub conversation_id: String,
    pub created_at: String,
}
