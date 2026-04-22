use serde::{Deserialize, Serialize};

/// Content type for post items
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    Text,
    Image,
    Video,
    File,
}

impl ContentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentType::Text => "text",
            ContentType::Image => "image",
            ContentType::Video => "video",
            ContentType::File => "file",
        }
    }
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A post in the feed - can contain multiple content items
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Post {
    pub post_id: String,
    pub author_id: String,
    pub is_published: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// Individual content item within a post
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PostContent {
    pub content_id: String,
    pub post_id: String,
    pub content_type: String, // stored as text in SQLite
    pub display_order: i32,
    /// For text: the text content
    /// For media: file path or storage key
    pub content: String,
    /// For media: thumbnail path (optional)
    pub thumbnail_path: Option<String>,
    /// Original filename for files
    pub original_filename: Option<String>,
    /// MIME type
    pub mime_type: Option<String>,
    /// File size in bytes (for media)
    pub file_size: Option<i64>,
    pub created_at: String,
    /// Hex-encoded AES-256-GCM nonce when content is encrypted at rest; None = plaintext.
    pub content_nonce: Option<String>,
}

/// Post with its content items and metadata for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostWithContent {
    pub post: Post,
    pub contents: Vec<PostContent>,
    pub comment_count: i64,
}

/// Summary for feed listing (without full content)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostSummary {
    pub post_id: String,
    pub author_id: String,
    pub preview_content_type: String,
    pub is_published: i32,
    pub comment_count: i64,
    pub created_at: String,
}

impl From<&Post> for PostSummary {
    fn from(post: &Post) -> Self {
        Self {
            post_id: post.post_id.clone(),
            author_id: post.author_id.clone(),
            preview_content_type: "text".to_string(), // default, should be set from first content
            is_published: post.is_published,
            comment_count: 0,
            created_at: post.created_at.clone(),
        }
    }
}
