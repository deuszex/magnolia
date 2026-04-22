use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::config::Settings;
use crate::utils::crypto::hash_password;
use crate::utils::password::validate_password_strength;
use magnolia_common::errors::AppError;
use magnolia_common::models::UserAccount;
use magnolia_common::repositories::UserRepository;
use magnolia_common::schemas::auth::MessageResponse;

type AppState = (sqlx::AnyPool, Arc<Settings>);

#[derive(Serialize)]
pub struct SetupStatusResponse {
    pub setup_required: bool,
}

#[derive(Deserialize)]
pub struct SetupRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

/// GET /api/setup/status
/// Returns whether the server needs initial setup (no users exist yet).
pub async fn setup_status(State((pool, _)): State<AppState>) -> impl IntoResponse {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM user_accounts WHERE user_id != '__fed__' AND user_id != '__proxy__'",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(0);

    Json(SetupStatusResponse {
        setup_required: count == 0,
    })
}

/// POST /api/setup
/// Create the initial admin account. Permanently disabled once any user exists.
pub async fn setup(
    State((pool, _)): State<AppState>,
    Json(payload): Json<SetupRequest>,
) -> Result<(StatusCode, Json<MessageResponse>), AppError> {
    // Guard: this endpoint is a one-time operation
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM user_accounts WHERE user_id != '__fed__' AND user_id != '__proxy__'",
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    if count > 0 {
        return Err(AppError::Forbidden);
    }

    let username = payload.username.trim().to_string();
    if username.len() < 3 {
        return Err(AppError::BadRequest(
            "Username must be at least 3 characters".to_string(),
        ));
    }

    let email = payload.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(AppError::BadRequest(
            "A valid email address is required".to_string(),
        ));
    }

    validate_password_strength(&payload.password)?;

    let password_hash = hash_password(&payload.password)?;
    let mut user = UserAccount::new(username.clone(), Some(email.clone()), password_hash);
    user.admin = 1;
    user.verified = 1;

    UserRepository::new(pool).create_user(&user).await?;

    tracing::info!("Initial admin account created: {} ({})", username, email);

    Ok((
        StatusCode::CREATED,
        Json(MessageResponse {
            message: "Admin account created successfully".to_string(),
        }),
    ))
}
