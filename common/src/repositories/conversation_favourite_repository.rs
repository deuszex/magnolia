use chrono::Utc;
use sqlx::AnyPool;

#[derive(Clone)]
pub struct ConversationFavouriteRepository {
    pool: AnyPool,
}

impl ConversationFavouriteRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    pub async fn add(&self, user_id: &str, conversation_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"INSERT OR IGNORE INTO conversation_favourites (user_id, conversation_id, created_at)
 VALUES ($1, $2, $3)"#,
        )
        .bind(user_id)
        .bind(conversation_id)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove(&self, user_id: &str, conversation_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"DELETE FROM conversation_favourites WHERE user_id = $1 AND conversation_id = $2"#,
        )
        .bind(user_id)
        .bind(conversation_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Returns the set of conversation_ids that this user has favourited.
    pub async fn list_for_user(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            r#"SELECT conversation_id FROM conversation_favourites WHERE user_id = $1"#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn is_favourite(
        &self,
        user_id: &str,
        conversation_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let row = sqlx::query_scalar::<_, i64>(
 r#"SELECT COUNT(*) FROM conversation_favourites WHERE user_id = $1 AND conversation_id = $2"#,
 )
 .bind(user_id)
 .bind(conversation_id)
 .fetch_one(&self.pool)
 .await?;
        Ok(row > 0)
    }
}
