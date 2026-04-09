//! Event creation and push helpers.
//!
//! `create_event` writes to `user_events` and immediately pushes a WebSocket
//! notification to the recipient if they are connected.
//!
//! `create_admin_event` broadcasts to every active admin.

use sqlx::AnyPool;
use tracing::error;

use crate::handlers::ws::{ConnectionRegistry, send_to_user};
use magnolia_common::repositories::EventRepository;

/// Write one event to the DB and push it via WebSocket to the recipient.
pub async fn create_event(
    pool: &AnyPool,
    registry: Option<&ConnectionRegistry>,
    user_id: &str,
    category: &str,
    event_type: &str,
    priority: &str,
    title: &str,
    body: &str,
    metadata: Option<serde_json::Value>,
) {
    let meta_str = metadata.as_ref().map(|m| m.to_string());
    let repo = EventRepository::new(pool.clone());

    let event = match repo
        .create(
            user_id,
            category,
            event_type,
            priority,
            title,
            body,
            meta_str.as_deref(),
        )
        .await
    {
        Ok(e) => e,
        Err(err) => {
            error!(
                "create_event: db insert failed for user {}: {:?}",
                user_id, err
            );
            return;
        }
    };

    if let Some(reg) = registry {
        let ws = serde_json::json!({
        "type": "event",
        "event": {
        "id": event.id,
        "user_id": event.user_id,
        "category": event.category,
        "event_type": event.event_type,
        "priority": event.priority,
        "title": event.title,
        "body": event.body,
        "metadata": meta_str,
        "viewed": false,
        "created_at": event.created_at,
        }
        });
        send_to_user(reg, user_id, &ws.to_string()).await;
    }
}

/// Write an event to every active admin, pushing via WebSocket for each.
pub async fn create_admin_event(
    pool: &AnyPool,
    registry: Option<&ConnectionRegistry>,
    category: &str,
    event_type: &str,
    priority: &str,
    title: &str,
    body: &str,
    metadata: Option<serde_json::Value>,
) {
    let repo = EventRepository::new(pool.clone());
    let admin_ids = match repo.get_admin_user_ids().await {
        Ok(ids) => ids,
        Err(e) => {
            error!("create_admin_event: could not fetch admins: {:?}", e);
            return;
        }
    };
    for admin_id in &admin_ids {
        create_event(
            pool,
            registry,
            admin_id,
            category,
            event_type,
            priority,
            title,
            body,
            metadata.clone(),
        )
        .await;
    }
}

/// Increment violation counter for a server connection.
/// Returns the new count.
pub async fn record_federation_violation(pool: &AnyPool, conn_id: &str) -> i64 {
    let now = chrono::Utc::now().to_rfc3339();
    let _ = sqlx::query(
        "UPDATE server_connections
 SET violation_count = violation_count + 1, last_violation_at = $1
 WHERE id = $2",
    )
    .bind(&now)
    .bind(conn_id)
    .execute(pool)
    .await;

    let count: (i64,) =
        sqlx::query_as("SELECT violation_count FROM server_connections WHERE id = $1")
            .bind(conn_id)
            .fetch_one(pool)
            .await
            .unwrap_or((0,));

    count.0
}
