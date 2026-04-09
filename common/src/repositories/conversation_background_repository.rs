use chrono::Utc;
use sqlx::AnyPool;

#[derive(Clone)]
pub struct ConversationBackgroundRepository {
    pool: AnyPool,
}

impl ConversationBackgroundRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    /// Get the media_id for a user's conversation background
    pub async fn get(
        &self,
        user_id: &str,
        conversation_id: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            r#"SELECT media_id FROM conversation_backgrounds
 WHERE user_id = $1 AND conversation_id = $2"#,
        )
        .bind(user_id)
        .bind(conversation_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Set (upsert) a conversation background for a user
    pub async fn set(
        &self,
        user_id: &str,
        conversation_id: &str,
        media_id: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
 r#"INSERT INTO conversation_backgrounds (user_id, conversation_id, media_id, created_at, updated_at)
 VALUES ($1, $2, $3, $4, $5)
 ON CONFLICT (user_id, conversation_id) DO UPDATE SET media_id = $3, updated_at = $5"#,
 )
 .bind(user_id)
 .bind(conversation_id)
 .bind(media_id)
 .bind(&now)
 .bind(&now)
 .execute(&self.pool)
 .await?;
        Ok(())
    }

    /// Remove a conversation background for a user
    pub async fn delete(&self, user_id: &str, conversation_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"DELETE FROM conversation_backgrounds WHERE user_id = $1 AND conversation_id = $2"#,
        )
        .bind(user_id)
        .bind(conversation_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
