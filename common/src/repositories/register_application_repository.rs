use chrono::Utc;
use sqlx::AnyPool;

use crate::errors::AppError;
use crate::models::RegisterApplication;

#[derive(Clone)]
pub struct RegisterApplicationRepository {
    pool: AnyPool,
}

impl RegisterApplicationRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, app: &RegisterApplication) -> Result<(), AppError> {
        sqlx::query(
 r#"INSERT INTO register_applications
 (application_id, username, email, password, display_name, message, status, created_at, expires_at, reviewed_at, reviewed_by)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
 )
 .bind(&app.application_id)
 .bind(&app.username)
 .bind(&app.email)
 .bind(&app.password)
 .bind(&app.display_name)
 .bind(&app.message)
 .bind(&app.status)
 .bind(&app.created_at)
 .bind(&app.expires_at)
 .bind(&app.reviewed_at)
 .bind(&app.reviewed_by)
 .execute(&self.pool)
 .await?;
        Ok(())
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<RegisterApplication>, AppError> {
        let app = sqlx::query_as::<_, RegisterApplication>(
 r#"SELECT application_id, username, email, password, display_name, message, status, created_at, expires_at, reviewed_at, reviewed_by
 FROM register_applications WHERE application_id = $1"#,
 )
 .bind(id)
 .fetch_optional(&self.pool)
 .await?;
        Ok(app)
    }

    /// Find the most recent pending application for a username
    pub async fn find_pending_by_username(
        &self,
        username: &str,
    ) -> Result<Option<RegisterApplication>, AppError> {
        let app = sqlx::query_as::<_, RegisterApplication>(
 r#"SELECT application_id, username, email, password, display_name, message, status, created_at, expires_at, reviewed_at, reviewed_by
 FROM register_applications
 WHERE username = $1 AND status = 'pending'
 ORDER BY created_at DESC LIMIT 1"#,
 )
 .bind(username)
 .fetch_optional(&self.pool)
 .await?;
        Ok(app)
    }

    /// Find the most recent pending application for an email
    pub async fn find_pending_by_email(
        &self,
        email: &str,
    ) -> Result<Option<RegisterApplication>, AppError> {
        let app = sqlx::query_as::<_, RegisterApplication>(
 r#"SELECT application_id, username, email, password, display_name, message, status, created_at, expires_at, reviewed_at, reviewed_by
 FROM register_applications
 WHERE email = $1 AND status = 'pending'
 ORDER BY created_at DESC LIMIT 1"#,
 )
 .bind(email)
 .fetch_optional(&self.pool)
 .await?;
        Ok(app)
    }

    /// List applications, optionally filtered by status.
    pub async fn list(
        &self,
        limit: i64,
        offset: i64,
        status: Option<&str>,
    ) -> Result<(Vec<RegisterApplication>, i64), AppError> {
        let apps = if let Some(s) = status {
            sqlx::query_as::<_, RegisterApplication>(
 r#"SELECT application_id, username, email, password, display_name, message, status, created_at, expires_at, reviewed_at, reviewed_by
 FROM register_applications WHERE status = $1
 ORDER BY created_at DESC LIMIT $2 OFFSET $3"#,
 )
 .bind(s)
 .bind(limit)
 .bind(offset)
 .fetch_all(&self.pool)
 .await?
        } else {
            sqlx::query_as::<_, RegisterApplication>(
 r#"SELECT application_id, username, email, password, display_name, message, status, created_at, expires_at, reviewed_at, reviewed_by
 FROM register_applications
 ORDER BY created_at DESC LIMIT $1 OFFSET $2"#,
 )
 .bind(limit)
 .bind(offset)
 .fetch_all(&self.pool)
 .await?
        };

        let total: (i64,) = if let Some(s) = status {
            sqlx::query_as("SELECT COUNT(*) FROM register_applications WHERE status = $1")
                .bind(s)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_as("SELECT COUNT(*) FROM register_applications")
                .fetch_one(&self.pool)
                .await?
        };

        Ok((apps, total.0))
    }

    pub async fn update_status(
        &self,
        id: &str,
        status: &str,
        reviewed_by: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
 "UPDATE register_applications SET status = $1, reviewed_by = $2, reviewed_at = $3 WHERE application_id = $4",
 )
 .bind(status)
 .bind(reviewed_by)
 .bind(&now)
 .bind(id)
 .execute(&self.pool)
 .await?;
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<(), AppError> {
        sqlx::query("DELETE FROM register_applications WHERE application_id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Mark all pending applications that have passed their expires_at as 'expired'.
    pub async fn expire_pending(&self) -> Result<u64, AppError> {
        let now = Utc::now().to_rfc3339();
        let result = sqlx::query(
 "UPDATE register_applications SET status = 'expired' WHERE status = 'pending' AND expires_at < $1",
 )
 .bind(&now)
 .execute(&self.pool)
 .await?;
        Ok(result.rows_affected())
    }
}
