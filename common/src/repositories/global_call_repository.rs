use sqlx::AnyPool;

use crate::models::GlobalCallParticipant;

#[derive(Clone)]
pub struct GlobalCallRepository {
    pool: AnyPool,
}

impl GlobalCallRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    /// List all current participants (joined_at ascending, oldest first).
    pub async fn list_participants(&self) -> Result<Vec<GlobalCallParticipant>, sqlx::Error> {
        sqlx::query_as::<_, GlobalCallParticipant>(
            "SELECT user_id, joined_at FROM global_call_participants ORDER BY joined_at ASC",
        )
        .fetch_all(&self.pool)
        .await
    }

    /// Returns true if the user is already a participant.
    pub async fn is_participant(&self, user_id: &str) -> Result<bool, sqlx::Error> {
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM global_call_participants WHERE user_id = $1")
                .bind(user_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(row.0 > 0)
    }

    /// Add a participant. No-op if already present (INSERT OR IGNORE for SQLite,
    /// INSERT … ON CONFLICT DO NOTHING for Postgres).
    pub async fn join(&self, user_id: &str, joined_at: &str) -> Result<(), sqlx::Error> {
        // AnyPool doesn't support INSERT OR IGNORE / ON CONFLICT in a portable way,
        // so we check first then insert.
        if self.is_participant(user_id).await? {
            return Ok(());
        }
        sqlx::query("INSERT INTO global_call_participants (user_id, joined_at) VALUES ($1, $2)")
            .bind(user_id)
            .bind(joined_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Remove a participant. No-op if not present.
    pub async fn leave(&self, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM global_call_participants WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
