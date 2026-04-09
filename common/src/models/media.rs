use serde::{Deserialize, Serialize};

/// Media type for gallery items
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Image,
    Video,
    File,
}

impl MediaType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaType::Image => "image",
            MediaType::Video => "video",
            MediaType::File => "file",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "image" => Some(MediaType::Image),
            "video" => Some(MediaType::Video),
            "file" => Some(MediaType::File),
            _ => None,
        }
    }
}

impl std::fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Media item in the gallery/repository
/// Standalone media that may or may not be attached to posts
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Media {
    pub media_id: String,
    pub owner_id: String,
    pub media_type: String, // image, video, file
    /// Storage path on disk
    pub storage_path: String,
    /// Thumbnail path (for images/videos)
    pub thumbnail_path: Option<String>,
    /// Original filename
    pub filename: String,
    /// MIME type (needed for serving)
    pub mime_type: String,
    /// File size in bytes
    pub file_size: i64,
    /// Duration in seconds (for video/audio)
    pub duration_seconds: Option<i32>,
    /// Image/video dimensions
    pub width: Option<i32>,
    pub height: Option<i32>,
    /// Hash of original file (for deduplication)
    pub file_hash: String,
    /// Optional description/caption
    pub description: Option<String>,
    /// Tags for organization (stored as JSON array)
    pub tags: Option<String>,
    /// Hex-encoded nonce for AES-256-GCM decryption (None if not encrypted at rest)
    pub encryption_nonce: Option<String>,
    pub is_deleted: i32,
    /// Base URL of the originating peer server. NULL for local uploads.
    pub origin_server: Option<String>,
    /// The media_id on the origin server. NULL for local uploads.
    pub origin_media_id: Option<String>,
    /// 0 = stub row, file not yet fetched from peer; 1 = file present locally (always 1 for local uploads)
    pub is_cached: i32,
    /// 1 = a background fetch is in progress; prevents duplicate concurrent fetches
    pub is_fetching: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// Media summary for gallery listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaSummary {
    pub media_id: String,
    pub media_type: String,
    pub mime_type: String,
    pub file_size: i64,
    pub duration_seconds: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub created_at: String,
}

impl From<&Media> for MediaSummary {
    fn from(media: &Media) -> Self {
        Self {
            media_id: media.media_id.clone(),
            media_type: media.media_type.clone(),
            mime_type: media.mime_type.clone(),
            file_size: media.file_size,
            duration_seconds: media.duration_seconds,
            width: media.width,
            height: media.height,
            created_at: media.created_at.clone(),
        }
    }
}

/// Gallery filter/query options
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MediaFilter {
    pub media_type: Option<String>,
    pub tags: Option<Vec<String>>,
    pub mime_type: Option<String>,
    pub min_size: Option<i64>,
    pub max_size: Option<i64>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}
