use sqlx::AnyPool;

use crate::models::{Call, CallParticipant};

#[derive(Clone)]
pub struct CallRepository {
    pool: AnyPool,
}

impl CallRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    // Calls

    pub async fn create(&self, call: &Call) -> Result<(), sqlx::Error> {
        sqlx::query(
 r#"INSERT INTO calls (call_id, conversation_id, initiated_by, call_type, status, started_at, ended_at, duration_seconds, created_at, is_open)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
 )
 .bind(&call.call_id)
 .bind(&call.conversation_id)
 .bind(&call.initiated_by)
 .bind(&call.call_type)
 .bind(&call.status)
 .bind(&call.started_at)
 .bind(&call.ended_at)
 .bind(&call.duration_seconds)
 .bind(&call.created_at)
 .bind(call.is_open)
 .execute(&self.pool)
 .await?;
        Ok(())
    }

    pub async fn get_by_id(&self, call_id: &str) -> Result<Option<Call>, sqlx::Error> {
        sqlx::query_as::<_, Call>(
            r#"SELECT call_id, conversation_id, initiated_by, call_type, status,
 started_at, ended_at, duration_seconds, created_at, is_open
 FROM calls WHERE call_id = $1"#,
        )
        .bind(call_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn update_status(&self, call_id: &str, status: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE calls SET status = $1 WHERE call_id = $2")
            .bind(status)
            .bind(call_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_started(&self, call_id: &str, started_at: &str) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE calls SET status = 'active', started_at = $1 WHERE call_id = $2")
            .bind(started_at)
            .bind(call_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_ended(
        &self,
        call_id: &str,
        ended_at: &str,
        duration: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
 "UPDATE calls SET status = 'ended', ended_at = $1, duration_seconds = $2 WHERE call_id = $3",
 )
 .bind(ended_at)
 .bind(duration)
 .bind(call_id)
 .execute(&self.pool)
 .await?;
        Ok(())
    }

    /// Get the active (ringing or active) call for a conversation, if any.
    pub async fn get_active_call(
        &self,
        conversation_id: &str,
    ) -> Result<Option<Call>, sqlx::Error> {
        sqlx::query_as::<_, Call>(
            r#"SELECT call_id, conversation_id, initiated_by, call_type, status,
 started_at, ended_at, duration_seconds, created_at, is_open
 FROM calls
 WHERE conversation_id = $1 AND status IN ('ringing', 'active')
 ORDER BY created_at DESC
 LIMIT 1"#,
        )
        .bind(conversation_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// List calls for a specific conversation (newest first).
    pub async fn list_for_conversation(
        &self,
        conversation_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<Call>, sqlx::Error> {
        sqlx::query_as::<_, Call>(
            r#"SELECT call_id, conversation_id, initiated_by, call_type, status,
 started_at, ended_at, duration_seconds, created_at, is_open
 FROM calls
 WHERE conversation_id = $1
 ORDER BY created_at DESC
 LIMIT $2 OFFSET $3"#,
        )
        .bind(conversation_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
    }

    /// List calls for a user across all conversations (newest first).
    pub async fn list_for_user(
        &self,
        user_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<Call>, sqlx::Error> {
        sqlx::query_as::<_, Call>(
            r#"SELECT DISTINCT c.call_id, c.conversation_id, c.initiated_by, c.call_type, c.status,
 c.started_at, c.ended_at, c.duration_seconds, c.created_at, c.is_open
 FROM calls c
 INNER JOIN call_participants cp ON cp.call_id = c.call_id
 WHERE cp.user_id = $1
 ORDER BY c.created_at DESC
 LIMIT $2 OFFSET $3"#,
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
    }

    /// Count calls for a user (for pagination).
    pub async fn count_for_user(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        let row: (i64,) = sqlx::query_as(
            r#"SELECT COUNT(DISTINCT c.call_id)
 FROM calls c
 INNER JOIN call_participants cp ON cp.call_id = c.call_id
 WHERE cp.user_id = $1"#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Count calls for a conversation (for pagination).
    pub async fn count_for_conversation(&self, conversation_id: &str) -> Result<i64, sqlx::Error> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM calls WHERE conversation_id = $1")
            .bind(conversation_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    // Participants

    pub async fn add_participant(&self, p: &CallParticipant) -> Result<(), sqlx::Error> {
        sqlx::query(
 r#"INSERT INTO call_participants (id, call_id, user_id, role, status, joined_at, left_at)
 VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
 )
 .bind(&p.id)
 .bind(&p.call_id)
 .bind(&p.user_id)
 .bind(&p.role)
 .bind(&p.status)
 .bind(&p.joined_at)
 .bind(&p.left_at)
 .execute(&self.pool)
 .await?;
        Ok(())
    }

    pub async fn update_participant_status(
        &self,
        call_id: &str,
        user_id: &str,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE call_participants SET status = $1 WHERE call_id = $2 AND user_id = $3")
            .bind(status)
            .bind(call_id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_participant_joined(
        &self,
        call_id: &str,
        user_id: &str,
        joined_at: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
 "UPDATE call_participants SET status = 'joined', joined_at = $1 WHERE call_id = $2 AND user_id = $3",
 )
 .bind(joined_at)
 .bind(call_id)
 .bind(user_id)
 .execute(&self.pool)
 .await?;
        Ok(())
    }

    pub async fn set_participant_left(
        &self,
        call_id: &str,
        user_id: &str,
        left_at: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
 "UPDATE call_participants SET status = 'left', left_at = $1 WHERE call_id = $2 AND user_id = $3",
 )
 .bind(left_at)
 .bind(call_id)
 .bind(user_id)
 .execute(&self.pool)
 .await?;
        Ok(())
    }

    pub async fn get_participant(
        &self,
        call_id: &str,
        user_id: &str,
    ) -> Result<Option<CallParticipant>, sqlx::Error> {
        sqlx::query_as::<_, CallParticipant>(
            r#"SELECT id, call_id, user_id, role, status, joined_at, left_at
 FROM call_participants WHERE call_id = $1 AND user_id = $2"#,
        )
        .bind(call_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn list_participants(
        &self,
        call_id: &str,
    ) -> Result<Vec<CallParticipant>, sqlx::Error> {
        sqlx::query_as::<_, CallParticipant>(
            r#"SELECT id, call_id, user_id, role, status, joined_at, left_at
 FROM call_participants WHERE call_id = $1"#,
        )
        .bind(call_id)
        .fetch_all(&self.pool)
        .await
    }

    /// Count participants who are currently joined (not left/rejected/missed).
    pub async fn count_active_participants(&self, call_id: &str) -> Result<i64, sqlx::Error> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM call_participants WHERE call_id = $1 AND status = 'joined'",
        )
        .bind(call_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Check if all non-initiator participants have rejected or missed.
    pub async fn all_participants_declined(&self, call_id: &str) -> Result<bool, sqlx::Error> {
        let row: (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*) FROM call_participants
 WHERE call_id = $1 AND role != 'initiator' AND status NOT IN ('rejected', 'missed')"#,
        )
        .bind(call_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0 == 0)
    }
}
