use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Call {
    pub call_id: String,
    pub conversation_id: String,
    pub initiated_by: String,
    pub call_type: String,
    pub status: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub duration_seconds: Option<i32>,
    pub created_at: String,
    /// 0 = private (invitation required), 1 = open (any conv member can join)
    pub is_open: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CallParticipant {
    pub id: String,
    pub call_id: String,
    pub user_id: String,
    pub role: String,
    pub status: String,
    pub joined_at: Option<String>,
    pub left_at: Option<String>,
}
