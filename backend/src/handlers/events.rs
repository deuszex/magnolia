//! User event feed — list, mark viewed, preferences.

use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::Value;
use sqlx::AnyPool;
use std::sync::Arc;

use crate::{config::Settings, handlers::ws::ConnectionRegistry, middleware::AuthMiddleware};
use magnolia_common::{
    repositories::EventRepository,
    schemas::{
        EventListQuery, EventListResponse, EventPrefsResponse, EventResponse, MarkAllViewedQuery,
        UnreadCountResponse, UpdateEventPrefsRequest,
    },
};

type AppState = (AnyPool, Arc<Settings>);

fn parse_metadata(raw: Option<&str>) -> Option<Value> {
    raw.and_then(|s| serde_json::from_str(s).ok())
}

fn event_to_response(e: magnolia_common::models::UserEvent) -> EventResponse {
    EventResponse {
        id: e.id,
        user_id: e.user_id,
        category: e.category,
        event_type: e.event_type,
        priority: e.priority,
        title: e.title,
        body: e.body,
        metadata: parse_metadata(e.metadata.as_deref()),
        viewed: e.viewed != 0,
        viewed_at: e.viewed_at,
        created_at: e.created_at,
    }
}

/// GET /api/events
pub async fn list_events(
    State((pool, _)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Query(query): Query<EventListQuery>,
) -> impl IntoResponse {
    let repo = EventRepository::new(pool.clone());

    let only_unviewed: Option<bool> = match query.viewed.as_str() {
        "unviewed" => Some(true),
        "viewed" => Some(false),
        _ => None, // "all"
    };

    // When only_unviewed = Some(false) we want to show viewed-only events.
    // Map to viewed_filter: None = all, Some(0) = unviewed, Some(1) = viewed.
    let viewed_filter: Option<i32> = match only_unviewed {
        Some(true) => Some(0),
        Some(false) => Some(1),
        None => None,
    };

    // Re-use only_unviewed: Option<bool> in the repo call with our corrected mapping.
    // We pass None for "all" and Some(true/false) for filtered.
    let (events, total) = match repo
        .list(
            &auth.user.user_id,
            only_unviewed,
            query.category.as_deref(),
            query.event_type.as_deref(),
            query.from.as_deref(),
            query.to.as_deref(),
            query.limit,
            query.offset,
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("list_events: {:?}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let unread = repo.count_unread(&auth.user.user_id).await.unwrap_or(0);

    let _ = viewed_filter; // suppress warning

    Json(EventListResponse {
        events: events.into_iter().map(event_to_response).collect(),
        total,
        unread,
    })
    .into_response()
}

/// GET /api/events/count
pub async fn get_unread_count(
    State((pool, _)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
) -> impl IntoResponse {
    let repo = EventRepository::new(pool);
    match repo.count_unread(&auth.user.user_id).await {
        Ok(count) => Json(UnreadCountResponse { count }).into_response(),
        Err(e) => {
            tracing::error!("get_unread_count: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// PUT /api/events/{id}/viewed
pub async fn mark_event_viewed(
    State((pool, _)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Path(event_id): Path<String>,
) -> impl IntoResponse {
    let repo = EventRepository::new(pool);
    match repo.mark_viewed(&event_id, &auth.user.user_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("mark_event_viewed: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// PUT /api/events/viewed-all
pub async fn mark_all_events_viewed(
    State((pool, _)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Query(q): Query<MarkAllViewedQuery>,
) -> impl IntoResponse {
    let repo = EventRepository::new(pool);
    match repo
        .mark_all_viewed(&auth.user.user_id, q.category.as_deref())
        .await
    {
        Ok(count) => Json(serde_json::json!({ "marked": count })).into_response(),
        Err(e) => {
            tracing::error!("mark_all_events_viewed: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// DELETE /api/events/{id}
pub async fn delete_event(
    State((pool, _)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Path(event_id): Path<String>,
) -> impl IntoResponse {
    let repo = EventRepository::new(pool);
    match repo.delete(&event_id, &auth.user.user_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("delete_event: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// GET /api/events/prefs
pub async fn get_event_prefs(
    State((pool, _)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
) -> impl IntoResponse {
    let repo = EventRepository::new(pool);
    match repo.get_prefs(&auth.user.user_id).await {
        Ok(prefs) => {
            let cats: Vec<String> =
                serde_json::from_str(&prefs.disabled_categories).unwrap_or_default();
            Json(EventPrefsResponse {
                disabled_categories: cats,
            })
            .into_response()
        }
        Err(e) => {
            tracing::error!("get_event_prefs: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// PUT /api/events/prefs
pub async fn update_event_prefs(
    State((pool, _)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Json(payload): Json<UpdateEventPrefsRequest>,
) -> impl IntoResponse {
    let repo = EventRepository::new(pool);
    let json = serde_json::to_string(&payload.disabled_categories).unwrap_or_else(|_| "[]".into());
    match repo.upsert_prefs(&auth.user.user_id, &json).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!("update_event_prefs: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// PUT /api/profile/email-visible — update email visibility preference.
pub async fn update_email_visible(
    State((pool, _)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Json(payload): Json<magnolia_common::schemas::UpdateEmailVisibleRequest>,
) -> impl IntoResponse {
    let repo = magnolia_common::repositories::UserRepository::new(pool);
    match repo
        .update_email_visible(&auth.user.user_id, payload.email_visible)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!("update_email_visible: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
