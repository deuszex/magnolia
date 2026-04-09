use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EmailSettings {
    pub id: i64,

    // SMTP Configuration
    pub smtp_host: String,
    pub smtp_port: i32,
    pub smtp_username: String,
    #[serde(skip_serializing_if = "should_mask_password")]
    pub smtp_password: String,
    pub smtp_from: String,
    pub smtp_secure: String, // 'tls', 'ssl', or 'none'

    // High-value purchase alerts
    pub high_value_enabled: i32,
    pub high_value_threshold: i64, // In cents
    pub high_value_recipient: String,

    // Pending delivery reminders
    pub pending_delivery_enabled: i32,
    pub pending_delivery_schedules: String, // JSON array of cron expressions
    pub pending_delivery_recipient: String,
    pub pending_delivery_include_products: i32,

    // Invoice email settings
    pub invoice_email_enabled: i32,
    pub invoice_email_trigger: String, // 'processing', 'shipped', or 'delivered'

    pub created_at: String,
    pub updated_at: String,
}

fn should_mask_password(_: &String) -> bool {
    // Always skip serializing the actual password in API responses
    true
}

impl EmailSettings {
    pub fn get_schedules(&self) -> Result<Vec<String>, serde_json::Error> {
        serde_json::from_str(&self.pending_delivery_schedules)
    }

    pub fn set_schedules(&mut self, schedules: &[String]) -> Result<(), serde_json::Error> {
        self.pending_delivery_schedules = serde_json::to_string(schedules)?;
        Ok(())
    }
}
