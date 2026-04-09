use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::collections::HashMap;

#[derive(Debug)]
pub enum AppError {
    // Client errors (4xx)
    Unauthorized,
    Forbidden,
    NotFound(String),
    BadRequest(String),
    Validation(HashMap<String, String>),
    Conflict(String),

    // Server errors (5xx)
    Database(sqlx::Error),
    Internal(String),
    EmailService(String),
    PaymentProvider(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "Forbidden".to_string()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Validation(errors) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                    "error": "Validation failed",
                    "status": 400,
                    "details": errors
                    })),
                )
                    .into_response();
            }
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            AppError::Database(err) => {
                tracing::error!(
                target: "security",
                event_category = "database",
                event_action = "database_error",
                event_outcome = "failure",
                error_type = "database",
                error_message = %err,
                "Database error occurred"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            AppError::Internal(msg) => {
                tracing::error!(
                target: "security",
                event_category = "application",
                event_action = "internal_error",
                event_outcome = "failure",
                error_type = "internal",
                error_message = %msg,
                "Internal error occurred"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            AppError::EmailService(msg) => {
                tracing::error!(
                target: "security",
                event_category = "email",
                event_action = "email_service_error",
                event_outcome = "failure",
                error_type = "email_service",
                error_message = %msg,
                "Email service error"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Email service unavailable".to_string(),
                )
            }
            AppError::PaymentProvider(msg) => {
                tracing::error!(
                target: "security",
                event_category = "payment",
                event_action = "payment_provider_error",
                event_outcome = "failure",
                error_type = "payment_provider",
                error_message = %msg,
                "Payment provider error"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Payment service unavailable".to_string(),
                )
            }
        };

        let body = Json(json!({
        "error": error_message,
        "status": status.as_u16()
        }));

        (status, body).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Database(err)
    }
}

impl From<validator::ValidationErrors> for AppError {
    fn from(err: validator::ValidationErrors) -> Self {
        let mut errors = HashMap::new();
        for (field, field_errors) in err.field_errors() {
            let messages: Vec<String> = field_errors
                .iter()
                .filter_map(|e| e.message.as_ref().map(|m| m.to_string()))
                .collect();
            errors.insert(field.to_string(), messages.join(", "));
        }
        AppError::Validation(errors)
    }
}
