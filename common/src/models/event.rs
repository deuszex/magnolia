use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserEvent {
    pub id: String,
    pub user_id: String,
    pub category: String,
    pub event_type: String,
    pub priority: String,
    pub title: String,
    pub body: String,
    pub metadata: Option<String>,
    pub viewed: i32,
    pub viewed_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserEventPrefs {
    pub user_id: String,
    /// JSON array of disabled category strings, e.g. ["social", "system"]
    pub disabled_categories: String,
}
