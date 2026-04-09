use serde::{Deserialize, Serialize};

/// Comment content type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommentContentType {
    Text,
    Image,
    Gif,
}

impl CommentContentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommentContentType::Text => "text",
            CommentContentType::Image => "image",
            CommentContentType::Gif => "gif",
        }
    }
}

impl std::fmt::Display for CommentContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A comment on a post
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Comment {
    pub comment_id: String,
    pub post_id: String,
    pub author_id: String,
    /// Parent comment ID for nested replies (null for top-level comments)
    pub parent_comment_id: Option<String>,
    pub content_type: String, // text, image, gif
    /// Comment text content
    pub content: String,
    /// For image/gif: media path
    pub media_path: Option<String>,
    pub is_deleted: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// Comment with author info for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentWithAuthor {
    pub comment: Comment,
    pub author_display_name: String,
    pub reply_count: i64,
}

/// Flattened comment for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentSummary {
    pub comment_id: String,
    pub post_id: String,
    pub author_id: String,
    pub author_display_name: String,
    pub parent_comment_id: Option<String>,
    pub content_type: String,
    pub is_deleted: i32,
    pub reply_count: i64,
    pub created_at: String,
}
