use chrono::Utc;
use sqlx::AnyPool;

use crate::errors::AppError;
use crate::models::{MessagingPreferences, UserBlock};

#[derive(Clone)]
pub struct MessagingRepository {
    pool: AnyPool,
}

impl MessagingRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    // Preferences

    pub async fn get_preferences(
        &self,
        user_id: &str,
    ) -> Result<Option<MessagingPreferences>, sqlx::Error> {
        sqlx::query_as::<_, MessagingPreferences>(
            r#"SELECT user_id, accept_messages, created_at, updated_at
 FROM messaging_preferences WHERE user_id = $1"#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Insert or replace messaging preferences (upsert).
    pub async fn upsert_preferences(
        &self,
        user_id: &str,
        accept_messages: i32,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"INSERT INTO messaging_preferences (user_id, accept_messages, created_at, updated_at)
 VALUES ($1, $2, $3, $4)
 ON CONFLICT (user_id) DO UPDATE SET accept_messages = $2, updated_at = $4"#,
        )
        .bind(user_id)
        .bind(accept_messages)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // Blocks

    pub async fn get_block(
        &self,
        user_id: &str,
        blocked_user_id: &str,
    ) -> Result<Option<UserBlock>, sqlx::Error> {
        sqlx::query_as::<_, UserBlock>(
            r#"SELECT id, user_id, blocked_user_id, created_at
 FROM user_blocks WHERE user_id = $1 AND blocked_user_id = $2"#,
        )
        .bind(user_id)
        .bind(blocked_user_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn list_blocks(&self, user_id: &str) -> Result<Vec<UserBlock>, sqlx::Error> {
        sqlx::query_as::<_, UserBlock>(
            r#"SELECT id, user_id, blocked_user_id, created_at
 FROM user_blocks WHERE user_id = $1 ORDER BY created_at DESC"#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn create_block(&self, block: &UserBlock) -> Result<(), AppError> {
        sqlx::query(
            r#"INSERT INTO user_blocks (id, user_id, blocked_user_id, created_at)
 VALUES ($1, $2, $3, $4)"#,
        )
        .bind(&block.id)
        .bind(&block.user_id)
        .bind(&block.blocked_user_id)
        .bind(&block.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.message().contains("UNIQUE") {
                    return AppError::Conflict("User is already blocked".to_string());
                }
            }
            AppError::Internal(format!("Failed to create block: {}", e))
        })?;
        Ok(())
    }

    pub async fn delete_block(
        &self,
        user_id: &str,
        blocked_user_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM user_blocks WHERE user_id = $1 AND blocked_user_id = $2"#)
            .bind(user_id)
            .bind(blocked_user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // Composite check

    /// Check whether `sender_id` is allowed to message `target_id`.
    /// Returns Ok(()) if allowed, or an appropriate AppError if not.
    pub async fn can_message(&self, sender_id: &str, target_id: &str) -> Result<(), AppError> {
        // Check accept_messages preference (default is accept)
        if let Some(prefs) = self.get_preferences(target_id).await.map_err(|e| {
            AppError::Internal(format!("Failed to check messaging preferences: {}", e))
        })? {
            if prefs.accept_messages == 0 {
                return Err(AppError::Forbidden);
            }
        }

        // Check blacklist
        let block = self
            .get_block(target_id, sender_id)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to check block list: {}", e)))?;
        if block.is_some() {
            return Err(AppError::Forbidden);
        }

        Ok(())
    }
}
