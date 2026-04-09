use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StunServer {
    pub id: String,
    pub url: String,
    pub username: Option<String>,
    pub credential: Option<String>,
    pub enabled: i32,
    pub last_checked_at: Option<String>,
    pub last_status: String,
    pub created_at: String,
    pub updated_at: String,
}
