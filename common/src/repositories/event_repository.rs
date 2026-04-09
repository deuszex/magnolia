use chrono::Utc;
use sqlx::AnyPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::{UserEvent, UserEventPrefs};

#[derive(Clone)]
pub struct EventRepository {
    pool: AnyPool,
}

impl EventRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    /// Insert a new event and return the populated row.
    pub async fn create(
        &self,
        user_id: &str,
        category: &str,
        event_type: &str,
        priority: &str,
        title: &str,
        body: &str,
        metadata: Option<&str>,
    ) -> Result<UserEvent, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
 "INSERT INTO user_events (id, user_id, category, event_type, priority, title, body, metadata, viewed, created_at)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 0, $9)",
 )
 .bind(&id)
 .bind(user_id)
 .bind(category)
 .bind(event_type)
 .bind(priority)
 .bind(title)
 .bind(body)
 .bind(metadata)
 .bind(&now)
 .execute(&self.pool)
 .await?;

        Ok(UserEvent {
            id,
            user_id: user_id.to_string(),
            category: category.to_string(),
            event_type: event_type.to_string(),
            priority: priority.to_string(),
            title: title.to_string(),
            body: body.to_string(),
            metadata: metadata.map(|s| s.to_string()),
            viewed: 0,
            viewed_at: None,
            created_at: now,
        })
    }

    /// List events for a user with optional filters.
    pub async fn list(
        &self,
        user_id: &str,
        only_unviewed: Option<bool>,
        category: Option<&str>,
        event_type: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<UserEvent>, i64), AppError> {
        // Build dynamic WHERE clause fragments using a base query + optional filters.
        // SQLx AnyPool doesn't support dynamic query building cleanly, so we use a fixed
        // maximum-filter query and bind NULLs for unused parameters.
        let events = sqlx::query_as::<_, UserEvent>(
            r#"
 SELECT id, user_id, category, event_type, priority, title, body,
 metadata, viewed, viewed_at, created_at
 FROM user_events
 WHERE user_id = $1
 AND ($2 IS NULL OR viewed = $2)
 AND ($3 IS NULL OR category = $3)
 AND ($4 IS NULL OR event_type = $4)
 AND ($5 IS NULL OR created_at >= $5)
 AND ($6 IS NULL OR created_at <= $6)
 ORDER BY created_at DESC
 LIMIT $7 OFFSET $8
 "#,
        )
        .bind(user_id)
        .bind(only_unviewed.map(|v| if v { 0i32 } else { 1i32 })) // 0 = unviewed, 1 = viewed
        .bind(category)
        .bind(event_type)
        .bind(from)
        .bind(to)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let total: (i64,) = sqlx::query_as(
            r#"
 SELECT COUNT(*) FROM user_events
 WHERE user_id = $1
 AND ($2 IS NULL OR viewed = $2)
 AND ($3 IS NULL OR category = $3)
 AND ($4 IS NULL OR event_type = $4)
 AND ($5 IS NULL OR created_at >= $5)
 AND ($6 IS NULL OR created_at <= $6)
 "#,
        )
        .bind(user_id)
        .bind(only_unviewed.map(|v| if v { 0i32 } else { 1i32 }))
        .bind(category)
        .bind(event_type)
        .bind(from)
        .bind(to)
        .fetch_one(&self.pool)
        .await?;

        Ok((events, total.0))
    }

    /// Count unread events for a user.
    pub async fn count_unread(&self, user_id: &str) -> Result<i64, AppError> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM user_events WHERE user_id = $1 AND viewed = 0")
                .bind(user_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(count.0)
    }

    /// Mark a single event as viewed.
    pub async fn mark_viewed(&self, event_id: &str, user_id: &str) -> Result<bool, AppError> {
        let now = Utc::now().to_rfc3339();
        let r = sqlx::query(
            "UPDATE user_events SET viewed = 1, viewed_at = $1 WHERE id = $2 AND user_id = $3",
        )
        .bind(&now)
        .bind(event_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }

    /// Mark all events as viewed, optionally filtered by category.
    pub async fn mark_all_viewed(
        &self,
        user_id: &str,
        category: Option<&str>,
    ) -> Result<u64, AppError> {
        let now = Utc::now().to_rfc3339();
        let r = sqlx::query(
            "UPDATE user_events SET viewed = 1, viewed_at = $1
 WHERE user_id = $2 AND viewed = 0
 AND ($3 IS NULL OR category = $3)",
        )
        .bind(&now)
        .bind(user_id)
        .bind(category)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected())
    }

    /// Delete a single event.
    pub async fn delete(&self, event_id: &str, user_id: &str) -> Result<bool, AppError> {
        let r = sqlx::query("DELETE FROM user_events WHERE id = $1 AND user_id = $2")
            .bind(event_id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    /// Get or create notification preferences for a user.
    pub async fn get_prefs(&self, user_id: &str) -> Result<UserEventPrefs, AppError> {
        let prefs = sqlx::query_as::<_, UserEventPrefs>(
            "SELECT user_id, disabled_categories FROM user_event_prefs WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(prefs.unwrap_or(UserEventPrefs {
            user_id: user_id.to_string(),
            disabled_categories: "[]".to_string(),
        }))
    }

    /// Upsert notification preferences.
    pub async fn upsert_prefs(
        &self,
        user_id: &str,
        disabled_categories: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO user_event_prefs (user_id, disabled_categories)
 VALUES ($1, $2)
 ON CONFLICT(user_id) DO UPDATE SET disabled_categories = excluded.disabled_categories",
        )
        .bind(user_id)
        .bind(disabled_categories)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Fetch all admin user IDs (for broadcast admin events).
    pub async fn get_admin_user_ids(&self) -> Result<Vec<String>, AppError> {
        let ids: Vec<(String,)> = sqlx::query_as(
 "SELECT user_id FROM user_accounts WHERE admin = 1 AND active = 1 AND user_id != '__fed__'",
 )
 .fetch_all(&self.pool)
 .await?;
        Ok(ids.into_iter().map(|(id,)| id).collect())
    }
}
