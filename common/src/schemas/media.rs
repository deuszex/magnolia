use serde::{Deserialize, Serialize};
use validator::Validate;

/// Request to upload media
#[derive(Debug, Deserialize, Validate)]
pub struct UploadMediaRequest {
    /// Media type: image, video, file
    #[validate(length(min = 1, max = 10))]
    pub media_type: String,
    /// Original filename
    #[validate(length(min = 1, max = 255))]
    pub filename: String,
    /// MIME type
    #[validate(length(min = 1, max = 100))]
    pub mime_type: String,
    /// File size in bytes (for validation)
    pub file_size: i64,
    /// Optional description/caption
    #[validate(length(max = 1000))]
    pub description: Option<String>,
    /// Optional tags for organization
    pub tags: Option<Vec<String>>,
}

/// Chunked upload initialization request
#[derive(Debug, Deserialize, Validate)]
pub struct InitChunkedUploadRequest {
    /// Media type: image, video, file
    #[validate(length(min = 1, max = 10))]
    pub media_type: String,
    /// Original filename
    #[validate(length(min = 1, max = 255))]
    pub filename: String,
    /// MIME type
    #[validate(length(min = 1, max = 100))]
    pub mime_type: String,
    /// Total file size
    pub total_size: i64,
    /// Chunk size being used
    pub chunk_size: i64,
}

/// Chunked upload initialization response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkedUploadResponse {
    pub upload_id: String,
    pub chunk_size: i64,
    pub total_chunks: i64,
}

/// Request to update media metadata
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateMediaRequest {
    /// Update description
    #[validate(length(max = 1000))]
    pub description: Option<String>,
    /// Update tags (replaces existing)
    pub tags: Option<Vec<String>>,
}

/// Query parameters for listing media (gallery)
#[derive(Debug, Default, Deserialize)]
pub struct ListMediaQuery {
    /// Filter by type: image, video, file
    pub media_type: Option<String>,
    /// Filter by tags (any match)
    pub tags: Option<String>, // comma-separated
    /// Filter by MIME type prefix (e.g., "image/", "video/mp4")
    pub mime_type: Option<String>,
    /// Minimum file size
    pub min_size: Option<i64>,
    /// Maximum file size
    pub max_size: Option<i64>,
    /// From date (ISO 8601)
    pub from_date: Option<String>,
    /// To date (ISO 8601)
    pub to_date: Option<String>,
    /// Search in filename/description
    pub search: Option<String>,
    /// Sort: newest, oldest, largest, smallest, name
    #[serde(default = "default_sort")]
    pub sort: String,
    /// Pagination limit
    #[serde(default = "default_limit")]
    pub limit: i32,
    /// Pagination offset
    #[serde(default)]
    pub offset: i32,
}

fn default_sort() -> String {
    "newest".to_string()
}

fn default_limit() -> i32 {
    50
}

/// Response for a single media item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItemResponse {
    pub media_id: String,
    pub media_type: String,
    pub filename: String,
    pub mime_type: String,
    pub file_size: i64,
    /// Direct URL to media
    pub url: String,
    /// Thumbnail URL (for images/videos)
    pub thumbnail_url: Option<String>,
    /// Duration in seconds (for video)
    pub duration_seconds: Option<i32>,
    /// Dimensions
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Paginated media list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaListResponse {
    pub items: Vec<MediaItemResponse>,
    pub total: i64,
    pub has_more: bool,
}

/// Media upload complete response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaUploadResponse {
    pub media_id: String,
    pub url: String,
    pub thumbnail_url: Option<String>,
}

/// Batch delete request
#[derive(Debug, Deserialize, Validate)]
pub struct BatchDeleteMediaRequest {
    #[validate(length(min = 1, max = 100))]
    pub media_ids: Vec<String>,
}

/// Batch operation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperationResponse {
    pub success_count: i32,
    pub failed_ids: Vec<String>,
}

/// Storage usage response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageUsageResponse {
    pub total_bytes: i64,
    pub image_bytes: i64,
    pub video_bytes: i64,
    pub file_bytes: i64,
    pub item_counts: StorageItemCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageItemCounts {
    pub images: i64,
    pub videos: i64,
    pub files: i64,
    pub total: i64,
}
