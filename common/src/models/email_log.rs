use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EmailLog {
    pub email_id: String,
    pub email_type: String,
    pub recipient: String,
    pub subject: String,
    pub sent_at: String,
    pub status: String, // 'sent' or 'failed'
    pub related_id: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub enum EmailType {
    Registration,
    VerificationResend,
    PasswordReset,
    PurchaseConfirmation,
    PurchaseInvoice,
    AdminHighValue,
    AdminPendingDelivery,
}

impl EmailType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EmailType::Registration => "registration",
            EmailType::VerificationResend => "verification_resend",
            EmailType::PasswordReset => "password_reset",
            EmailType::PurchaseConfirmation => "purchase_confirmation",
            EmailType::PurchaseInvoice => "purchase_invoice",
            EmailType::AdminHighValue => "admin_high_value",
            EmailType::AdminPendingDelivery => "admin_pending_delivery",
        }
    }
}
