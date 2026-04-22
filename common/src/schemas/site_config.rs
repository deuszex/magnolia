use serde::{Deserialize, Serialize};
use validator::Validate;

/// Request to update site configuration (partial update - all fields optional)
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateSiteConfigRequest {
    #[validate(length(max = 500))]
    pub media_storage_path: Option<String>,

    pub allow_text_posts: Option<bool>,
    pub allow_image_posts: Option<bool>,
    pub allow_video_posts: Option<bool>,
    pub allow_file_posts: Option<bool>,

    pub encryption_at_rest_enabled: Option<bool>,

    pub proxy_user_system: Option<bool>,
    /// Default max media uploads per minute for all proxies (pieces)
    #[validate(range(min = 1, max = 1000))]
    pub proxy_rate_limit_pieces: Option<i32>,
    /// Default max media upload bytes per minute for all proxies
    #[validate(range(min = 1))]
    pub proxy_rate_limit_bytes: Option<i32>,

    pub message_auto_delete_enabled: Option<bool>,
    #[validate(range(min = 0, max = 8760))]
    pub message_auto_delete_delay_hours: Option<i32>,

    /// 'open', 'invite_only', or 'application'
    pub registration_mode: Option<String>,
    #[validate(range(min = 1, max = 8760))]
    pub application_timeout_hours: Option<i32>,
    /// Require registration email to match the email an invite was sent to
    pub enforce_invite_email: Option<bool>,

    /// Allow email-based password reset (requires SMTP configured)
    pub password_reset_email_enabled: Option<bool>,
    /// Allow signing-key-based password reset
    pub password_reset_signing_key_enabled: Option<bool>,
}

/// Response for site configuration
#[derive(Debug, Serialize)]
pub struct SiteConfigResponse {
    pub media_storage_path: String,

    pub allow_text_posts: bool,
    pub allow_image_posts: bool,
    pub allow_video_posts: bool,
    pub allow_file_posts: bool,

    pub encryption_at_rest_enabled: bool,
    /// Whether the encryption key is configured in the environment
    pub encryption_key_configured: bool,

    pub message_auto_delete_enabled: bool,
    pub message_auto_delete_delay_hours: i32,

    pub registration_mode: String,
    pub application_timeout_hours: i32,
    /// When true, an invite with a bound email requires registration with that exact address
    pub enforce_invite_email: bool,
    /// Whether SMTP is configured in the server environment
    pub smtp_configured: bool,

    pub password_reset_email_enabled: bool,
    pub password_reset_signing_key_enabled: bool,

    pub proxy_user_system: bool,
    pub proxy_rate_limit_pieces: i32,
    pub proxy_rate_limit_bytes: i32,

    pub updated_at: String,
}
