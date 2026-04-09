use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PostTag {
    pub post_id: String,
    pub tag: String,
}
