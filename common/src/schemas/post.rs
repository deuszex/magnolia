use serde::{Deserialize, Serialize};
use validator::Validate;

/// Request to create a new post
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreatePostRequest {
    /// Content items for the post
    #[validate(length(min = 1, max = 20, message = "Post must have 1-20 content items"))]
    pub contents: Vec<CreatePostContentRequest>,
    /// Whether to publish immediately
    #[serde(default)]
    pub publish: bool,
    /// Tags for the post
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Individual content item in a post creation request
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreatePostContentRequest {
    /// Content type: text, image, video, file
    #[validate(length(min = 1, max = 10))]
    pub content_type: String,
    /// Display order (0-indexed)
    pub display_order: i32,
    /// For text: the text content (will be encrypted server-side)
    /// For media: base64 encoded file or media_id reference
    pub content: String,
    /// For media uploads: original filename
    pub filename: Option<String>,
    /// For media uploads: MIME type
    pub mime_type: Option<String>,
    /// For media: reference to existing media_id instead of upload
    pub media_id: Option<String>,
}

/// Request to update a post
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdatePostRequest {
    /// Updated content items (replaces all existing)
    #[validate(length(min = 1, max = 20, message = "Post must have 1-20 content items"))]
    pub contents: Option<Vec<CreatePostContentRequest>>,
    /// Update publish status
    pub publish: Option<bool>,
    /// Update tags (replaces all existing)
    pub tags: Option<Vec<String>>,
}

/// Query parameters for listing posts
#[derive(Debug, Default, Deserialize)]
pub struct ListPostsQuery {
    /// Filter by author
    pub author_id: Option<String>,
    /// Include unpublished (only for own posts)
    #[serde(default)]
    pub include_drafts: bool,
    /// Pagination: number of items
    #[serde(default = "default_limit")]
    pub limit: i32,
    /// Pagination: offset
    #[serde(default)]
    pub offset: i32,
    /// Filter by content type (any content of this type)
    pub content_type: Option<String>,
    /// Cursor-based pagination: post_id to start after
    pub after: Option<String>,
}

fn default_limit() -> i32 {
    20
}

/// Response for a single post with contents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostResponse {
    pub post_id: String,
    pub author_id: String,
    pub contents: Vec<PostContentResponse>,
    pub tags: Vec<String>,
    pub is_published: bool,
    pub comment_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Response for a post content item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostContentResponse {
    pub content_id: String,
    pub content_type: String,
    pub display_order: i32,
    /// Decrypted content (text or media URL)
    pub content: String,
    /// Thumbnail URL for media
    pub thumbnail_url: Option<String>,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub file_size: Option<i64>,
}

/// Summary response for feed listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostSummaryResponse {
    pub post_id: String,
    pub author_id: String,
    pub author_name: Option<String>,
    pub author_avatar_url: Option<String>,
    /// All content items for the post
    pub contents: Vec<PostContentResponse>,
    pub tags: Vec<String>,
    pub is_published: bool,
    pub comment_count: i64,
    pub created_at: String,
    /// Set for federated posts: the originating server's base URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_server: Option<String>,
}

/// Paginated list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostListResponse {
    pub posts: Vec<PostSummaryResponse>,
    pub total: i64,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

/// Query parameters for searching posts
#[derive(Debug, Default, Deserialize)]
pub struct SearchPostsQuery {
    /// Text to search for in post content
    pub q: Option<String>,
    /// Comma-separated tags to filter by
    pub tags: Option<String>,
    /// Filter: posts with image content
    #[serde(default)]
    pub has_images: bool,
    /// Filter: posts with video content
    #[serde(default)]
    pub has_videos: bool,
    /// Filter: posts with file content
    #[serde(default)]
    pub has_files: bool,
    /// Date range start (RFC3339)
    pub from_date: Option<String>,
    /// Date range end (RFC3339)
    pub to_date: Option<String>,
    /// Filter by author
    pub author_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i32,
    #[serde(default)]
    pub offset: i32,
}
