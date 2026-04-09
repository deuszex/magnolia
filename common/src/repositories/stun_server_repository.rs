use sqlx::AnyPool;

use crate::errors::AppError;
use crate::models::StunServer;

#[derive(Clone)]
pub struct StunServerRepository {
    pool: AnyPool,
}

impl StunServerRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    pub async fn list_all(&self) -> Result<Vec<StunServer>, AppError> {
        let rows = sqlx::query_as::<_, StunServer>(
            "SELECT id, url, username, credential, enabled, last_checked_at, last_status, \
             created_at, updated_at FROM stun_servers ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn list_enabled(&self) -> Result<Vec<StunServer>, AppError> {
        let rows = sqlx::query_as::<_, StunServer>(
            "SELECT id, url, username, credential, enabled, last_checked_at, last_status, \
             created_at, updated_at FROM stun_servers WHERE enabled = 1 ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get_by_id(&self, id: &str) -> Result<Option<StunServer>, AppError> {
        let row = sqlx::query_as::<_, StunServer>(
            "SELECT id, url, username, credential, enabled, last_checked_at, last_status, \
             created_at, updated_at FROM stun_servers WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn create(&self, server: &StunServer) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO stun_servers \
             (id, url, username, credential, enabled, last_status, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(&server.id)
        .bind(&server.url)
        .bind(&server.username)
        .bind(&server.credential)
        .bind(server.enabled)
        .bind(&server.last_status)
        .bind(&server.created_at)
        .bind(&server.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update(
        &self,
        id: &str,
        url: &str,
        username: Option<&str>,
        credential: Option<&str>,
        enabled: i32,
        updated_at: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE stun_servers SET url = $1, username = $2, credential = $3, \
             enabled = $4, updated_at = $5 WHERE id = $6",
        )
        .bind(url)
        .bind(username)
        .bind(credential)
        .bind(enabled)
        .bind(updated_at)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_health(
        &self,
        id: &str,
        status: &str,
        checked_at: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            "UPDATE stun_servers SET last_status = $1, last_checked_at = $2, \
             updated_at = $2 WHERE id = $3",
        )
        .bind(status)
        .bind(checked_at)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<(), AppError> {
        sqlx::query("DELETE FROM stun_servers WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
