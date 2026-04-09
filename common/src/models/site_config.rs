use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Site-wide configuration settings (singleton row, id=1)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SiteConfig {
    pub id: i64,

    // -- Data storage --
    pub media_storage_path: String,

    // -- Allowed post content types --
    pub allow_text_posts: i32,
    pub allow_image_posts: i32,
    pub allow_video_posts: i32,
    pub allow_file_posts: i32,

    // -- Encryption at rest --
    pub encryption_at_rest_enabled: i32,

    // -- Message auto-deletion (preparatory for E2E DMs) --
    pub message_auto_delete_enabled: i32,
    pub message_auto_delete_delay_hours: i32,

    // -- Registration --
    /// 'open', 'invite_only', or 'application'
    pub registration_mode: String,
    pub application_timeout_hours: i32,
    /// When true, an invite with a bound email requires registration with that exact address
    pub enforce_invite_email: i32,

    // -- Metadata --
    pub created_at: String,
    pub updated_at: String,
}

impl SiteConfig {
    /// Check if a given content type string is allowed
    pub fn is_content_type_allowed(&self, content_type: &str) -> bool {
        match content_type {
            "text" => self.allow_text_posts == 1,
            "image" => self.allow_image_posts == 1,
            "video" => self.allow_video_posts == 1,
            "file" => self.allow_file_posts == 1,
            _ => false,
        }
    }

    /// Returns the effective media storage path, falling back to the default
    pub fn effective_storage_path(&self) -> &str {
        if self.media_storage_path.is_empty() {
            "./media_storage"
        } else {
            &self.media_storage_path
        }
    }
}
