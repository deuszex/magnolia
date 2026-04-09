use chrono::Utc;
use sqlx::AnyPool;

use crate::models::EmailSettings;

#[derive(Clone)]
pub struct EmailSettingsRepository {
    pool: AnyPool,
}

impl EmailSettingsRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    /// Get email settings (singleton)
    pub async fn get(&self) -> Result<EmailSettings, sqlx::Error> {
        let settings = sqlx::query_as::<_, EmailSettings>(
            r#"
 SELECT
 id,
 smtp_host,
 smtp_port,
 smtp_username,
 smtp_password,
 smtp_from,
 smtp_secure,
 high_value_enabled,
 high_value_threshold,
 high_value_recipient,
 pending_delivery_enabled,
 pending_delivery_schedules,
 pending_delivery_recipient,
 pending_delivery_include_products,
 invoice_email_enabled,
 invoice_email_trigger,
 created_at,
 updated_at
 FROM email_settings
 WHERE id = 1
 "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(settings)
    }

    /// Update email settings
    pub async fn update(&self, settings: &EmailSettings) -> Result<(), sqlx::Error> {
        let updated_at = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
 UPDATE email_settings
 SET
 smtp_host = $1,
 smtp_port = $2,
 smtp_username = $3,
 smtp_password = $4,
 smtp_from = $5,
 smtp_secure = $6,
 high_value_enabled = $7,
 high_value_threshold = $8,
 high_value_recipient = $9,
 pending_delivery_enabled = $10,
 pending_delivery_schedules = $11,
 pending_delivery_recipient = $12,
 pending_delivery_include_products = $13,
 invoice_email_enabled = $14,
 invoice_email_trigger = $15,
 updated_at = $16
 WHERE id = 1
 "#,
        )
        .bind(&settings.smtp_host)
        .bind(settings.smtp_port)
        .bind(&settings.smtp_username)
        .bind(&settings.smtp_password)
        .bind(&settings.smtp_from)
        .bind(&settings.smtp_secure)
        .bind(settings.high_value_enabled)
        .bind(settings.high_value_threshold)
        .bind(&settings.high_value_recipient)
        .bind(settings.pending_delivery_enabled)
        .bind(&settings.pending_delivery_schedules)
        .bind(&settings.pending_delivery_recipient)
        .bind(settings.pending_delivery_include_products)
        .bind(settings.invoice_email_enabled)
        .bind(&settings.invoice_email_trigger)
        .bind(&updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
