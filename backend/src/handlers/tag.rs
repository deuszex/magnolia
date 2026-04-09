use axum::{Json, response::IntoResponse};

use magnolia_common::errors::AppError;
use magnolia_common::repositories::PostTagRepository;
use magnolia_common::schemas::{TagInfo, TagListResponse};

use crate::config::Settings;
use axum::extract::State;
use sqlx::AnyPool;
use std::sync::Arc;

type AppState = (AnyPool, Arc<Settings>);

/// List all tags with usage counts (for autocomplete / discovery).
/// GET /api/tags
pub async fn list_tags(
    State((pool, _settings)): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let repo = PostTagRepository::new(pool);
    let tag_counts = repo.list_all_tags().await.map_err(|e| {
        tracing::error!("Failed to list tags: {:?}", e);
        AppError::Internal("Failed to list tags".to_string())
    })?;

    let tags: Vec<TagInfo> = tag_counts
        .into_iter()
        .map(|tc| TagInfo {
            name: tc.tag,
            count: tc.count,
        })
        .collect();

    Ok(Json(TagListResponse { tags }))
}
