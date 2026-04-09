use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Invite {
    pub invite_id: String,
    pub token: String,
    pub email: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub expires_at: String,
    pub used_at: Option<String>,
    pub used_by_user_id: Option<String>,
}

impl Invite {
    pub fn new(created_by: String, email: Option<String>, expires_hours: i64) -> Self {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::hours(expires_hours);
        Self {
            invite_id: uuid::Uuid::new_v4().to_string(),
            token: uuid::Uuid::new_v4().to_string(),
            email,
            created_by,
            created_at: now.to_rfc3339(),
            expires_at: expires_at.to_rfc3339(),
            used_at: None,
            used_by_user_id: None,
        }
    }

    pub fn is_expired(&self) -> bool {
        if let Ok(expires) = DateTime::parse_from_rfc3339(&self.expires_at) {
            expires.with_timezone(&Utc) < Utc::now()
        } else {
            true
        }
    }
}
