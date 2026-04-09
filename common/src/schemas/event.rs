use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventResponse {
    pub id: String,
    pub user_id: String,
    pub category: String,
    pub event_type: String,
    pub priority: String,
    pub title: String,
    pub body: String,
    pub metadata: Option<serde_json::Value>,
    pub viewed: bool,
    pub viewed_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventListResponse {
    pub events: Vec<EventResponse>,
    pub total: i64,
    pub unread: i64,
}

#[derive(Debug, Deserialize)]
pub struct EventListQuery {
    /// "unviewed" | "viewed" | "all" — defaults to "unviewed"
    #[serde(default = "default_viewed_filter")]
    pub viewed: String,
    pub category: Option<String>,
    pub event_type: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_viewed_filter() -> String {
    "unviewed".to_string()
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Deserialize)]
pub struct MarkAllViewedQuery {
    pub category: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventPrefsResponse {
    pub disabled_categories: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEventPrefsRequest {
    pub disabled_categories: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UnreadCountResponse {
    pub count: i64,
}
