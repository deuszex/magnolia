use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// A user's request to join the platform (used when registration_mode = 'application')
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RegisterApplication {
    pub application_id: String,
    pub email: String,
    pub display_name: Option<String>,
    /// Optional motivation message from the applicant
    pub message: Option<String>,
    /// 'pending', 'approved', 'denied', 'expired'
    pub status: String,
    pub created_at: String,
    pub expires_at: String,
    pub reviewed_at: Option<String>,
    pub reviewed_by: Option<String>,
}

impl RegisterApplication {
    pub fn new(
        email: String,
        display_name: Option<String>,
        message: Option<String>,
        timeout_hours: i64,
    ) -> Self {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::hours(timeout_hours);
        Self {
            application_id: format!("app-{}", Uuid::new_v4()),
            email,
            display_name,
            message,
            status: "pending".to_string(),
            created_at: now.to_rfc3339(),
            expires_at: expires_at.to_rfc3339(),
            reviewed_at: None,
            reviewed_by: None,
        }
    }

    pub fn is_expired(&self) -> bool {
        if let Ok(expires) = self.expires_at.parse::<DateTime<Utc>>() {
            Utc::now() > expires
        } else {
            false
        }
    }
}
