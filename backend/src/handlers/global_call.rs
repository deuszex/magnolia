use axum::{Extension, Json, extract::State};
use chrono::Utc;
use sqlx::AnyPool;
use std::sync::Arc;

use crate::config::Settings;
use crate::handlers::ws::{ConnectionRegistry, send_to_user};
use crate::middleware::auth::AuthMiddleware;
use magnolia_common::errors::AppError;
use magnolia_common::repositories::{GlobalCallRepository, UserRepository};
use magnolia_common::schemas::{GlobalCallParticipantResponse, GlobalCallResponse};

type AppState = (AnyPool, Arc<Settings>);

/// Build the participant list, enriching with display names from the user table.
async fn build_response(pool: &AnyPool) -> Result<GlobalCallResponse, AppError> {
    let repo = GlobalCallRepository::new(pool.clone());
    let user_repo = UserRepository::new(pool.clone());

    let participants = repo.list_participants().await.map_err(|e| {
        AppError::Internal(format!("Failed to list global call participants: {}", e))
    })?;

    let mut result = Vec::with_capacity(participants.len());
    for p in participants {
        let display_name = user_repo
            .find_by_id(&p.user_id)
            .await
            .ok()
            .flatten()
            .and_then(|u| u.display_name);
        result.push(GlobalCallParticipantResponse {
            user_id: p.user_id,
            display_name,
            joined_at: p.joined_at,
        });
    }

    Ok(GlobalCallResponse {
        participants: result,
    })
}

/// Broadcast a `global_call_update` to all currently connected users.
pub async fn broadcast_global_call_update(
    pool: &AnyPool,
    registry: &ConnectionRegistry,
) -> Result<(), AppError> {
    let response = build_response(pool).await?;
    let msg = serde_json::json!({
    "type": "global_call_update",
    "participants": response.participants,
    })
    .to_string();

    // Send to every connected user
    let user_ids: Vec<String> = {
        let reg = registry.read().await;
        reg.keys().cloned().collect()
    };
    for uid in user_ids {
        send_to_user(registry, &uid, &msg).await;
    }
    Ok(())
}

/// GET /api/global-call
pub async fn get_global_call(
    State((pool, _)): State<AppState>,
    Extension(_auth): Extension<AuthMiddleware>,
) -> Result<Json<GlobalCallResponse>, AppError> {
    Ok(Json(build_response(&pool).await?))
}

/// POST /api/global-call/join
pub async fn join_global_call(
    State((pool, _)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Extension(registry): Extension<ConnectionRegistry>,
) -> Result<Json<GlobalCallResponse>, AppError> {
    let repo = GlobalCallRepository::new(pool.clone());
    let now = Utc::now().to_rfc3339();
    repo.join(&auth.user.user_id, &now)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to join global call: {}", e)))?;

    broadcast_global_call_update(&pool, &registry).await?;
    Ok(Json(build_response(&pool).await?))
}

/// POST /api/global-call/leave
pub async fn leave_global_call(
    State((pool, _)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Extension(registry): Extension<ConnectionRegistry>,
) -> Result<Json<GlobalCallResponse>, AppError> {
    let repo = GlobalCallRepository::new(pool.clone());
    repo.leave(&auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to leave global call: {}", e)))?;

    broadcast_global_call_update(&pool, &registry).await?;
    Ok(Json(build_response(&pool).await?))
}
