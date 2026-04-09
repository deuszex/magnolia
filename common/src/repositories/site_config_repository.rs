use chrono::Utc;
use sqlx::AnyPool;

use crate::models::SiteConfig;

#[derive(Clone)]
pub struct SiteConfigRepository {
    pool: AnyPool,
}

impl SiteConfigRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    /// Get site configuration (singleton, id=1)
    pub async fn get(&self) -> Result<SiteConfig, sqlx::Error> {
        sqlx::query_as::<_, SiteConfig>(
            r#"
 SELECT id, media_storage_path, allow_text_posts, allow_image_posts,
 allow_video_posts, allow_file_posts, encryption_at_rest_enabled,
 message_auto_delete_enabled, message_auto_delete_delay_hours,
 registration_mode, application_timeout_hours, enforce_invite_email,
 created_at, updated_at
 FROM site_config
 WHERE id = 1
 "#,
        )
        .fetch_one(&self.pool)
        .await
    }

    /// Update site configuration (partial update - merges with current values)
    pub async fn update(
        &self,
        media_storage_path: Option<&str>,
        allow_text_posts: Option<i32>,
        allow_image_posts: Option<i32>,
        allow_video_posts: Option<i32>,
        allow_file_posts: Option<i32>,
        encryption_at_rest_enabled: Option<i32>,
        message_auto_delete_enabled: Option<i32>,
        message_auto_delete_delay_hours: Option<i32>,
        registration_mode: Option<&str>,
        application_timeout_hours: Option<i32>,
        enforce_invite_email: Option<i32>,
    ) -> Result<SiteConfig, sqlx::Error> {
        let current = self.get().await?;
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
 UPDATE site_config
 SET media_storage_path = $1,
 allow_text_posts = $2,
 allow_image_posts = $3,
 allow_video_posts = $4,
 allow_file_posts = $5,
 encryption_at_rest_enabled = $6,
 message_auto_delete_enabled = $7,
 message_auto_delete_delay_hours = $8,
 registration_mode = $9,
 application_timeout_hours = $10,
 enforce_invite_email = $11,
 updated_at = $12
 WHERE id = 1
 "#,
        )
        .bind(media_storage_path.unwrap_or(&current.media_storage_path))
        .bind(allow_text_posts.unwrap_or(current.allow_text_posts))
        .bind(allow_image_posts.unwrap_or(current.allow_image_posts))
        .bind(allow_video_posts.unwrap_or(current.allow_video_posts))
        .bind(allow_file_posts.unwrap_or(current.allow_file_posts))
        .bind(encryption_at_rest_enabled.unwrap_or(current.encryption_at_rest_enabled))
        .bind(message_auto_delete_enabled.unwrap_or(current.message_auto_delete_enabled))
        .bind(message_auto_delete_delay_hours.unwrap_or(current.message_auto_delete_delay_hours))
        .bind(registration_mode.unwrap_or(&current.registration_mode))
        .bind(application_timeout_hours.unwrap_or(current.application_timeout_hours))
        .bind(enforce_invite_email.unwrap_or(current.enforce_invite_email))
        .bind(&now)
        .execute(&self.pool)
        .await?;

        self.get().await
    }
}
