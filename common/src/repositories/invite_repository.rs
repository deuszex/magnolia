use sqlx::AnyPool;

use crate::errors::AppError;
use crate::models::Invite;

#[derive(Clone)]
pub struct InviteRepository {
    pool: AnyPool,
}

impl InviteRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, invite: &Invite) -> Result<(), AppError> {
        sqlx::query(
 r#"INSERT INTO invites (invite_id, token, email, created_by, created_at, expires_at, used_at, used_by_user_id)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
 )
 .bind(&invite.invite_id)
 .bind(&invite.token)
 .bind(&invite.email)
 .bind(&invite.created_by)
 .bind(&invite.created_at)
 .bind(&invite.expires_at)
 .bind(&invite.used_at)
 .bind(&invite.used_by_user_id)
 .execute(&self.pool)
 .await?;
        Ok(())
    }

    pub async fn find_by_token(&self, token: &str) -> Result<Option<Invite>, AppError> {
        let invite = sqlx::query_as::<_, Invite>(
 r#"SELECT invite_id, token, email, created_by, created_at, expires_at, used_at, used_by_user_id
 FROM invites WHERE token = $1"#,
 )
 .bind(token)
 .fetch_optional(&self.pool)
 .await?;
        Ok(invite)
    }

    pub async fn find_by_id(&self, invite_id: &str) -> Result<Option<Invite>, AppError> {
        let invite = sqlx::query_as::<_, Invite>(
 r#"SELECT invite_id, token, email, created_by, created_at, expires_at, used_at, used_by_user_id
 FROM invites WHERE invite_id = $1"#,
 )
 .bind(invite_id)
 .fetch_optional(&self.pool)
 .await?;
        Ok(invite)
    }

    pub async fn list_all(&self, limit: i64, offset: i64) -> Result<(Vec<Invite>, i64), AppError> {
        let invites = sqlx::query_as::<_, Invite>(
 r#"SELECT invite_id, token, email, created_by, created_at, expires_at, used_at, used_by_user_id
 FROM invites ORDER BY created_at DESC LIMIT $1 OFFSET $2"#,
 )
 .bind(limit)
 .bind(offset)
 .fetch_all(&self.pool)
 .await?;

        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM invites")
            .fetch_one(&self.pool)
            .await?;

        Ok((invites, total.0))
    }

    pub async fn mark_used(
        &self,
        token: &str,
        used_by_user_id: &str,
        used_at: &str,
    ) -> Result<(), AppError> {
        sqlx::query("UPDATE invites SET used_at = $1, used_by_user_id = $2 WHERE token = $3")
            .bind(used_at)
            .bind(used_by_user_id)
            .bind(token)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete(&self, invite_id: &str) -> Result<(), AppError> {
        sqlx::query("DELETE FROM invites WHERE invite_id = $1")
            .bind(invite_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
