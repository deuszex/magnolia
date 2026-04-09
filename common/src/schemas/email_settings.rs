use serde::{Deserialize, Serialize};
use validator::Validate;

/// Response for email settings — password is never included
#[derive(Debug, Serialize)]
pub struct EmailSettingsResponse {
    pub smtp_host: String,
    pub smtp_port: i32,
    pub smtp_username: String,
    /// True if a password is stored (content is never returned)
    pub smtp_password_set: bool,
    pub smtp_from: String,
    /// 'tls', 'ssl', or 'none'
    pub smtp_secure: String,
    pub updated_at: String,
}

/// Request to update email settings (partial — omitted fields keep their current value)
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateEmailSettingsRequest {
    #[validate(length(max = 255))]
    pub smtp_host: Option<String>,
    #[validate(range(min = 1, max = 65535))]
    pub smtp_port: Option<i32>,
    #[validate(length(max = 255))]
    pub smtp_username: Option<String>,
    /// Provide a non-empty string to change the password; omit or send empty to keep existing
    #[validate(length(max = 1024))]
    pub smtp_password: Option<String>,
    #[validate(length(max = 320))]
    pub smtp_from: Option<String>,
    /// Must be 'tls', 'ssl', or 'none'
    #[validate(length(max = 10))]
    pub smtp_secure: Option<String>,
}
