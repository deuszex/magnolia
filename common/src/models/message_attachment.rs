use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MessageAttachment {
    pub id: String,
    pub message_id: String,
    pub media_id: String,
}
