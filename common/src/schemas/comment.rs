use serde::{Deserialize, Serialize};
use validator::Validate;

/// Request to create a comment
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateCommentRequest {
    /// Content type: text, image, gif
    #[validate(length(min = 1, max = 10))]
    pub content_type: String,
    /// For text: the comment text
    /// For image/gif: base64 encoded data or URL
    #[validate(length(min = 1, max = 10000))]
    pub content: String,
    /// Parent comment ID for replies
    pub parent_comment_id: Option<String>,
    /// For image uploads: original filename
    pub filename: Option<String>,
    /// For image uploads: MIME type
    pub mime_type: Option<String>,
}

/// Request to update a comment (only text content can be edited)
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateCommentRequest {
    #[validate(length(min = 1, max = 10000))]
    pub content: String,
}

/// Query parameters for listing comments
#[derive(Debug, Default, Deserialize)]
pub struct ListCommentsQuery {
    /// Filter by parent (null for top-level only)
    pub parent_comment_id: Option<String>,
    /// Include all nested replies
    #[serde(default)]
    pub include_replies: bool,
    /// Pagination limit
    #[serde(default = "default_limit")]
    pub limit: i32,
    /// Pagination offset
    #[serde(default)]
    pub offset: i32,
    /// Sort order: newest, oldest
    #[serde(default = "default_sort")]
    pub sort: String,
}

fn default_limit() -> i32 {
    50
}

fn default_sort() -> String {
    "newest".to_string()
}

/// Response for a single comment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentResponse {
    pub comment_id: String,
    pub post_id: String,
    pub author_id: String,
    pub author_display_name: String,
    pub author_avatar_url: Option<String>,
    pub parent_comment_id: Option<String>,
    pub content_type: String,
    /// Decrypted content
    pub content: String,
    /// Media URL if image/video/file
    pub media_url: Option<String>,
    /// Media ID if this is a media comment
    pub media_id: Option<String>,
    /// Original filename for file attachments
    pub filename: Option<String>,
    pub is_deleted: bool,
    pub reply_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Comment with nested replies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentWithReplies {
    #[serde(flatten)]
    pub comment: CommentResponse,
    pub replies: Vec<CommentResponse>,
}

/// Paginated comment list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentListResponse {
    pub comments: Vec<CommentResponse>,
    pub total: i64,
    pub has_more: bool,
}

/// Comment count response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentCountResponse {
    pub post_id: String,
    pub count: i64,
}
