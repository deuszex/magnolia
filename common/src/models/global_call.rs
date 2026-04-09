use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GlobalCallParticipant {
    pub user_id: String,
    pub joined_at: String,
}
