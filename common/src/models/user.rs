use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserAccount {
    pub user_id: String,
    /// Nullable — users may register without an email address.
    pub email: Option<String>,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub verified: i32,
    pub admin: i32,
    pub active: i32,
    pub display_name: Option<String>,
    pub username: String,
    pub bio: Option<String>,
    pub avatar_media_id: Option<String>,
    pub location: Option<String>,
    pub website: Option<String>,
    pub public_key: Option<String>,
    /// Passphrase-encrypted E2E private key blob (opaque to the server).
    /// Format: JSON { v, salt, iv, data, pub } — all fields base64-encoded except pub (JWK).
    pub e2e_key_blob: Option<String>,
    /// 1 = email shown to other authenticated users; 0 = private (default)
    pub email_visible: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl UserAccount {
    pub fn new(username: String, email: Option<String>, password_hash: String) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            user_id: uuid::Uuid::new_v4().to_string(),
            email,
            password_hash,
            verified: 0,
            admin: 0,
            active: 1,
            display_name: None,
            username,
            bio: None,
            avatar_media_id: None,
            location: None,
            website: None,
            public_key: None,
            e2e_key_blob: None,
            email_visible: 0,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Session {
    pub session_id: String,
    pub user_id: String,
    pub expires_at: String,
    pub created_at: String,
}

impl Session {
    pub fn new(user_id: String, duration_days: i64) -> Self {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::days(duration_days);

        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            user_id,
            expires_at: expires_at.to_rfc3339(),
            created_at: now.to_rfc3339(),
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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EmailVerification {
    pub token: String,
    pub user_id: String,
    pub expires_at: String,
    pub used: i32,
    pub created_at: String,
}

impl EmailVerification {
    pub fn new(user_id: String) -> Self {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::hours(24);

        Self {
            token: uuid::Uuid::new_v4().to_string(),
            user_id,
            expires_at: expires_at.to_rfc3339(),
            used: 0,
            created_at: now.to_rfc3339(),
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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PasswordReset {
    pub token: String,
    pub user_id: String,
    pub expires_at: String,
    pub used: i32,
    pub created_at: String,
}

impl PasswordReset {
    pub fn new(user_id: String) -> Self {
        let now = Utc::now();
        // Password reset tokens expire in 1 hour for security
        let expires_at = now + chrono::Duration::hours(1);

        Self {
            token: uuid::Uuid::new_v4().to_string(),
            user_id,
            expires_at: expires_at.to_rfc3339(),
            used: 0,
            created_at: now.to_rfc3339(),
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
