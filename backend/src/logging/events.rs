/// Security event types for XDR logging
/// Following Elastic Common Schema (ECS) conventions
#[derive(Debug, Clone, Copy)]
pub enum SecurityEvent {
    // Authentication events
    LoginSuccess,
    LoginFailure,
    Logout,
    RegistrationSuccess,
    RegistrationFailure,
    PasswordResetRequested,
    PasswordResetCompleted,
    EmailVerified,
    SessionExpired,
    SessionInvalid,

    // Authorization events
    AdminAccessGranted,
    AdminAccessDenied,
    UnauthorizedAccess,

    // Rate limiting events
    RateLimitExceeded,

    // Webhook events
    WebhookReceived,
    WebhookSignatureValid,
    WebhookSignatureInvalid,
    WebhookProcessed,
    WebhookFailed,

    // Payment events
    OrphanedTransaction,
    PaymentCompleted,
    PaymentFailed,
    PaymentRefunded,
    PaymentExpired,

    // Admin actions
    AdminConfigChange,
    AdminUserManagement,
    AuditReportGenerated,

    // Errors
    DatabaseError,
    InternalError,
    PaymentProviderError,
    EmailServiceError,
}

impl SecurityEvent {
    /// Get ECS event category
    pub fn category(&self) -> &'static str {
        match self {
            Self::LoginSuccess
            | Self::LoginFailure
            | Self::Logout
            | Self::RegistrationSuccess
            | Self::RegistrationFailure
            | Self::PasswordResetRequested
            | Self::PasswordResetCompleted
            | Self::EmailVerified
            | Self::SessionExpired
            | Self::SessionInvalid => "authentication",

            Self::AdminAccessGranted | Self::AdminAccessDenied | Self::UnauthorizedAccess => {
                "authorization"
            }

            Self::RateLimitExceeded => "intrusion_detection",

            Self::WebhookReceived
            | Self::WebhookSignatureValid
            | Self::WebhookSignatureInvalid
            | Self::WebhookProcessed
            | Self::WebhookFailed => "web",

            Self::OrphanedTransaction
            | Self::PaymentCompleted
            | Self::PaymentFailed
            | Self::PaymentRefunded
            | Self::PaymentExpired => "payment",

            Self::AdminConfigChange | Self::AdminUserManagement | Self::AuditReportGenerated => {
                "configuration"
            }

            Self::DatabaseError
            | Self::InternalError
            | Self::PaymentProviderError
            | Self::EmailServiceError => "error",
        }
    }

    /// Get ECS event action
    pub fn action(&self) -> &'static str {
        match self {
            Self::LoginSuccess | Self::LoginFailure => "login",
            Self::Logout => "logout",
            Self::RegistrationSuccess | Self::RegistrationFailure => "registration",
            Self::PasswordResetRequested => "password_reset_request",
            Self::PasswordResetCompleted => "password_reset",
            Self::EmailVerified => "email_verification",
            Self::SessionExpired => "session_expired",
            Self::SessionInvalid => "session_invalid",
            Self::AdminAccessGranted | Self::AdminAccessDenied => "admin_access",
            Self::UnauthorizedAccess => "access_denied",
            Self::RateLimitExceeded => "rate_limit_exceeded",
            Self::WebhookReceived => "webhook_received",
            Self::WebhookSignatureValid | Self::WebhookSignatureInvalid => "webhook_signature",
            Self::WebhookProcessed => "webhook_processed",
            Self::WebhookFailed => "webhook_failed",
            Self::OrphanedTransaction => "orphaned_transaction",
            Self::AdminConfigChange => "config_change",
            Self::AdminUserManagement => "user_management",
            Self::AuditReportGenerated => "audit_report",
            Self::PaymentCompleted => "payment_completed",
            Self::PaymentFailed => "payment_failed",
            Self::PaymentRefunded => "payment_refunded",
            Self::PaymentExpired => "payment_expired",
            Self::DatabaseError => "database_error",
            Self::InternalError => "internal_error",
            Self::PaymentProviderError => "payment_provider_error",
            Self::EmailServiceError => "email_service_error",
        }
    }

    /// Get ECS event outcome
    pub fn outcome(&self) -> &'static str {
        match self {
            Self::LoginSuccess
            | Self::RegistrationSuccess
            | Self::PasswordResetCompleted
            | Self::EmailVerified
            | Self::AdminAccessGranted
            | Self::WebhookSignatureValid
            | Self::WebhookProcessed
            | Self::AuditReportGenerated
            | Self::PaymentCompleted
            | Self::PaymentRefunded => "success",

            Self::LoginFailure
            | Self::RegistrationFailure
            | Self::AdminAccessDenied
            | Self::UnauthorizedAccess
            | Self::RateLimitExceeded
            | Self::WebhookSignatureInvalid
            | Self::WebhookFailed
            | Self::DatabaseError
            | Self::InternalError
            | Self::PaymentFailed
            | Self::PaymentProviderError
            | Self::EmailServiceError => "failure",

            Self::Logout
            | Self::PasswordResetRequested
            | Self::SessionExpired
            | Self::SessionInvalid
            | Self::WebhookReceived
            | Self::OrphanedTransaction
            | Self::AdminConfigChange
            | Self::AdminUserManagement
            | Self::PaymentExpired => "unknown",
        }
    }

    /// Get event kind (event, alert, metric)
    pub fn kind(&self) -> &'static str {
        match self {
            Self::RateLimitExceeded | Self::WebhookSignatureInvalid | Self::OrphanedTransaction => {
                "alert"
            }
            _ => "event",
        }
    }
}

impl std::fmt::Display for SecurityEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Macro for logging security events with ECS-compatible fields
#[macro_export]
macro_rules! security_log {
 // Basic event with message
 ($event:expr, $msg:expr) => {
 tracing::info!(
 target: "security",
 event_category = $event.category(),
 event_action = $event.action(),
 event_outcome = $event.outcome(),
 event_kind = $event.kind(),
 $msg
 )
 };
 // Event with additional fields
 ($event:expr, $msg:expr, $($key:ident = $value:expr),+ $(,)?) => {
 tracing::info!(
 target: "security",
 event_category = $event.category(),
 event_action = $event.action(),
 event_outcome = $event.outcome(),
 event_kind = $event.kind(),
 $($key = %$value,)+
 $msg
 )
 };
}

/// Macro for logging security warnings
#[macro_export]
macro_rules! security_warn {
 ($event:expr, $msg:expr) => {
 tracing::warn!(
 target: "security",
 event_category = $event.category(),
 event_action = $event.action(),
 event_outcome = $event.outcome(),
 event_kind = $event.kind(),
 $msg
 )
 };
 ($event:expr, $msg:expr, $($key:ident = $value:expr),+ $(,)?) => {
 tracing::warn!(
 target: "security",
 event_category = $event.category(),
 event_action = $event.action(),
 event_outcome = $event.outcome(),
 event_kind = $event.kind(),
 $($key = %$value,)+
 $msg
 )
 };
}

/// Macro for logging security errors
#[macro_export]
macro_rules! security_error {
 ($event:expr, $msg:expr) => {
 tracing::error!(
 target: "security",
 event_category = $event.category(),
 event_action = $event.action(),
 event_outcome = $event.outcome(),
 event_kind = $event.kind(),
 $msg
 )
 };
 ($event:expr, $msg:expr, $($key:ident = $value:expr),+ $(,)?) => {
 tracing::error!(
 target: "security",
 event_category = $event.category(),
 event_action = $event.action(),
 event_outcome = $event.outcome(),
 event_kind = $event.kind(),
 $($key = %$value,)+
 $msg
 )
 };
}
