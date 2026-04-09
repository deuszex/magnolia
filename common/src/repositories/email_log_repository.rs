use chrono::Utc;
use sqlx::AnyPool;
use uuid::Uuid;

use crate::models::{EmailLog, EmailType};

pub struct EmailLogRepository {
    pool: AnyPool,
}

impl EmailLogRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    /// Create a new email log entry
    pub async fn create(
        &self,
        email_type: EmailType,
        recipient: &str,
        subject: &str,
        status: &str,
        related_id: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<EmailLog, sqlx::Error> {
        let email_id = Uuid::new_v4().to_string();
        let sent_at = Utc::now().to_rfc3339();
        let created_at = sent_at.clone();
        let email_type_str = email_type.as_str();

        sqlx::query(
            r#"
 INSERT INTO email_logs (
 email_id,
 email_type,
 recipient,
 subject,
 sent_at,
 status,
 related_id,
 error_message,
 created_at
 )
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
 "#,
        )
        .bind(&email_id)
        .bind(email_type_str)
        .bind(recipient)
        .bind(subject)
        .bind(&sent_at)
        .bind(status)
        .bind(related_id)
        .bind(error_message)
        .bind(&created_at)
        .execute(&self.pool)
        .await?;

        Ok(EmailLog {
            email_id,
            email_type: email_type_str.to_string(),
            recipient: recipient.to_string(),
            subject: subject.to_string(),
            sent_at,
            status: status.to_string(),
            related_id: related_id.map(|s| s.to_string()),
            error_message: error_message.map(|s| s.to_string()),
            created_at,
        })
    }

    /// Get email logs by type
    pub async fn find_by_type(
        &self,
        email_type: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EmailLog>, sqlx::Error> {
        let logs = sqlx::query_as::<_, EmailLog>(
            r#"
 SELECT
 email_id,
 email_type,
 recipient,
 subject,
 sent_at,
 status,
 related_id,
 error_message,
 created_at
 FROM email_logs
 WHERE email_type = $1
 ORDER BY sent_at DESC
 LIMIT $2 OFFSET $3
 "#,
        )
        .bind(email_type)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(logs)
    }

    /// Get email logs by related ID
    pub async fn find_by_related_id(&self, related_id: &str) -> Result<Vec<EmailLog>, sqlx::Error> {
        let logs = sqlx::query_as::<_, EmailLog>(
            r#"
 SELECT
 email_id,
 email_type,
 recipient,
 subject,
 sent_at,
 status,
 related_id,
 error_message,
 created_at
 FROM email_logs
 WHERE related_id = $1
 ORDER BY sent_at DESC
 "#,
        )
        .bind(related_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(logs)
    }

    /// Get recent failed emails
    pub async fn find_failed(&self, limit: i64) -> Result<Vec<EmailLog>, sqlx::Error> {
        let logs = sqlx::query_as::<_, EmailLog>(
            r#"
 SELECT
 email_id,
 email_type,
 recipient,
 subject,
 sent_at,
 status,
 related_id,
 error_message,
 created_at
 FROM email_logs
 WHERE status = 'failed'
 ORDER BY sent_at DESC
 LIMIT $1
 "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(logs)
    }
}
