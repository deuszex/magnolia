//! Media handlers for uploading, viewing, and managing gallery items

use axum::{
    Json,
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::AnyPool;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;
use validator::Validate;

use crate::config::Settings;
use crate::middleware::auth::AuthMiddleware;
use crate::utils::encryption::ContentEncryption;
use crate::utils::thumbnail;
use magnolia_common::errors::AppError;
use magnolia_common::models::{Media, MediaFilter};
use magnolia_common::repositories::{MediaRepository, SiteConfigRepository};
use magnolia_common::schemas::{
    BatchDeleteMediaRequest, BatchOperationResponse, ChunkedUploadResponse,
    InitChunkedUploadRequest, ListMediaQuery, MediaItemResponse, MediaListResponse,
    MediaUploadResponse, StorageItemCounts, StorageUsageResponse, UpdateMediaRequest,
};

type AppState = (AnyPool, Arc<Settings>);

/// Get the configured media storage base path, falling back to "./media_storage".
pub async fn get_storage_base_pub(pool: &AnyPool) -> String {
    get_storage_base(pool).await
}

async fn get_storage_base(pool: &AnyPool) -> String {
    let repo = SiteConfigRepository::new(pool.clone());
    match repo.get().await {
        Ok(config) => config.effective_storage_path().to_string(),
        Err(_) => "./media_storage".to_string(),
    }
}

/// If encryption at rest is enabled, encrypt file data in-place on disk.
/// Returns the hex-encoded nonce, or None if encryption is not enabled.
async fn encrypt_file_on_disk(
    file_path: &str,
    settings: &Settings,
    pool: &AnyPool,
) -> Result<Option<String>, AppError> {
    let repo = SiteConfigRepository::new(pool.clone());
    let config = match repo.get().await {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };

    if config.encryption_at_rest_enabled != 1 {
        return Ok(None);
    }

    let key = settings.encryption_at_rest_key.as_deref().ok_or_else(|| {
        AppError::Internal("Encryption enabled but ENCRYPTION_AT_REST_KEY not set".to_string())
    })?;

    let enc = ContentEncryption::from_hex_key(key)?;
    let plaintext = tokio::fs::read(file_path).await.map_err(|e| {
        tracing::error!("Failed to read file for encryption: {:?}", e);
        AppError::Internal("Failed to encrypt file".to_string())
    })?;

    let (encrypted, nonce) = enc.encrypt_content(&plaintext)?;

    tokio::fs::write(file_path, &encrypted).await.map_err(|e| {
        tracing::error!("Failed to write encrypted file: {:?}", e);
        AppError::Internal("Failed to encrypt file".to_string())
    })?;

    Ok(Some(hex::encode(&nonce)))
}

/// Decrypt file data if encryption nonce is present.
fn decrypt_data(data: Vec<u8>, nonce_hex: &str, settings: &Settings) -> Result<Vec<u8>, AppError> {
    let key = settings.encryption_at_rest_key.as_deref().ok_or_else(|| {
        AppError::Internal("Encrypted file but ENCRYPTION_AT_REST_KEY not set".to_string())
    })?;
    let enc = ContentEncryption::from_hex_key(key)?;
    let nonce_bytes = hex::decode(nonce_hex)
        .map_err(|_| AppError::Internal("Invalid stored nonce".to_string()))?;
    enc.decrypt_content(&data, &nonce_bytes)
}

/// Upload a media file
/// POST /api/media
pub async fn upload_media(
    State((pool, settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    mut multipart: Multipart,
) -> Result<Json<MediaUploadResponse>, AppError> {
    let owner_id = auth.user.user_id;

    let repo = MediaRepository::new(pool.clone());
    let now = Utc::now().to_rfc3339();
    let media_id = Uuid::new_v4().to_string();

    // Parse multipart form
    let mut file_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut content_type: Option<String> = None;
    let mut media_type: Option<String> = None;
    let mut description: Option<String> = None;
    let mut tags: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        tracing::error!("Failed to parse multipart: {:?}", e);
        AppError::BadRequest("Invalid multipart data".to_string())
    })? {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "file" => {
                filename = field.file_name().map(|s| s.to_string());
                content_type = field.content_type().map(|s| s.to_string());
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| {
                            tracing::error!("Failed to read file data: {:?}", e);
                            AppError::BadRequest("Failed to read file".to_string())
                        })?
                        .to_vec(),
                );
            }
            "media_type" => {
                media_type = Some(field.text().await.unwrap_or_default());
            }
            "description" => {
                description = Some(field.text().await.unwrap_or_default());
            }
            "tags" => {
                tags = Some(field.text().await.unwrap_or_default());
            }
            _ => {}
        }
    }

    let file_data = file_data.ok_or(AppError::BadRequest("No file provided".to_string()))?;
    let filename = filename.unwrap_or_else(|| format!("{}.bin", media_id));
    let content_type = content_type.unwrap_or_else(|| "application/octet-stream".to_string());
    let media_type = media_type.unwrap_or_else(|| {
        if content_type.starts_with("image/") {
            "image".to_string()
        } else if content_type.starts_with("video/") {
            "video".to_string()
        } else {
            "file".to_string()
        }
    });

    // Calculate file hash
    let mut hasher = Sha256::new();
    hasher.update(&file_data);
    let file_hash = hex::encode(hasher.finalize());

    // Check for duplicate
    if let Some(existing) = repo.get_by_hash(&owner_id, &file_hash).await.map_err(|e| {
        tracing::error!("Failed to check for duplicate: {:?}", e);
        AppError::Internal("Failed to check for duplicate".to_string())
    })? {
        // Return existing media
        return Ok(Json(MediaUploadResponse {
            media_id: existing.media_id.clone(),
            url: format!("/api/media/{}/file", existing.media_id),
            thumbnail_url: Some(format!("/api/media/{}/thumbnail", existing.media_id)),
        }));
    }

    let file_size = file_data.len() as i64;

    let base = get_storage_base(&pool).await;
    let storage_path = format!("{}/{}", base, media_id);
    tokio::fs::create_dir_all(&base).await.map_err(|e| {
        tracing::error!("Failed to create storage directory: {:?}", e);
        AppError::Internal("Failed to store file".to_string())
    })?;

    let mut file = tokio::fs::File::create(&storage_path).await.map_err(|e| {
        tracing::error!("Failed to create file: {:?}", e);
        AppError::Internal("Failed to store file".to_string())
    })?;

    file.write_all(&file_data).await.map_err(|e| {
        tracing::error!("Failed to write file: {:?}", e);
        AppError::Internal("Failed to store file".to_string())
    })?;

    // Generate thumbnail and get dimensions BEFORE encrypting
    let thumbnail_dir = format!("{}/thumbnails", base);
    let thumbnail_path = match media_type.as_str() {
        "image" => {
            thumbnail::generate_image_thumbnail(&storage_path, &thumbnail_dir, &media_id).await
        }
        "video" => {
            thumbnail::generate_video_thumbnail(&storage_path, &thumbnail_dir, &media_id).await
        }
        _ => None,
    };

    let (width, height) = if media_type == "image" {
        let sp = storage_path.clone();
        tokio::task::spawn_blocking(move || image::image_dimensions(&sp).ok())
            .await
            .ok()
            .flatten()
            .map(|(w, h)| (Some(w as i32), Some(h as i32)))
            .unwrap_or((None, None))
    } else {
        (None, None)
    };

    // Optionally encrypt the file at rest (after thumbnail generation)
    let encryption_nonce = encrypt_file_on_disk(&storage_path, &settings, &pool).await?;

    let media = Media {
        media_id: media_id.clone(),
        owner_id,
        media_type: media_type.clone(),
        storage_path,
        thumbnail_path,
        filename,
        mime_type: content_type,
        file_size,
        duration_seconds: None,
        width,
        height,
        file_hash,
        description,
        tags,
        encryption_nonce,
        is_deleted: 0,
        origin_server: None,
        origin_media_id: None,
        is_cached: 1,
        is_fetching: 0,
        created_at: now.clone(),
        updated_at: now,
        proxy_owner_id: None,
    };

    repo.create(&media).await.map_err(|e| {
        tracing::error!("Failed to create media record: {:?}", e);
        AppError::Internal("Failed to store media".to_string())
    })?;

    Ok(Json(MediaUploadResponse {
        media_id: media_id.clone(),
        url: format!("/api/media/{}/file", media_id),
        thumbnail_url: Some(format!("/api/media/{}/thumbnail", media_id)),
    }))
}

/// Initialize chunked upload for large files
/// POST /api/media/chunked/init
pub async fn init_chunked_upload(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Json(payload): Json<InitChunkedUploadRequest>,
) -> Result<Json<ChunkedUploadResponse>, AppError> {
    payload.validate().map_err(AppError::from)?;

    let upload_id = Uuid::new_v4().to_string();
    let chunk_size = payload.chunk_size;
    let total_chunks = (payload.total_size + chunk_size - 1) / chunk_size;

    // Create upload directory for chunks
    let base = get_storage_base(&pool).await;
    let upload_dir = format!("{}/chunks/{}", base, upload_id);
    tokio::fs::create_dir_all(&upload_dir).await.map_err(|e| {
        tracing::error!("Failed to create chunk directory: {:?}", e);
        AppError::Internal("Failed to initialize upload".to_string())
    })?;

    // Write metadata file
    let metadata = serde_json::json!({
    "upload_id": upload_id,
    "owner_id": auth.user.user_id,
    "media_type": payload.media_type,
    "filename": payload.filename,
    "mime_type": payload.mime_type,
    "total_size": payload.total_size,
    "chunk_size": chunk_size,
    "total_chunks": total_chunks,
    "created_at": Utc::now().to_rfc3339(),
    });

    let metadata_path = format!("{}/metadata.json", upload_dir);
    tokio::fs::write(&metadata_path, serde_json::to_string(&metadata).unwrap())
        .await
        .map_err(|e| {
            tracing::error!("Failed to write upload metadata: {:?}", e);
            AppError::Internal("Failed to initialize upload".to_string())
        })?;

    Ok(Json(ChunkedUploadResponse {
        upload_id,
        chunk_size,
        total_chunks,
    }))
}

/// Upload a chunk
/// POST /api/media/chunked/:upload_id/:chunk_number
pub async fn upload_chunk(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path((upload_id, chunk_number)): Path<(String, i32)>,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, AppError> {
    let base = get_storage_base(&pool).await;
    let upload_dir = format!("{}/chunks/{}", base, upload_id);

    // Validate upload exists and belongs to user
    let metadata_path = format!("{}/metadata.json", upload_dir);
    let metadata_str = tokio::fs::read_to_string(&metadata_path)
        .await
        .map_err(|_| AppError::NotFound("Upload not found".to_string()))?;

    let metadata: serde_json::Value = serde_json::from_str(&metadata_str)
        .map_err(|_| AppError::Internal("Corrupt upload metadata".to_string()))?;

    if metadata["owner_id"].as_str() != Some(&auth.user.user_id) {
        return Err(AppError::Forbidden);
    }

    let total_chunks = metadata["total_chunks"].as_i64().unwrap_or(0);
    if chunk_number < 0 || chunk_number as i64 >= total_chunks {
        return Err(AppError::BadRequest("Invalid chunk number".to_string()));
    }

    // Write chunk to disk
    let chunk_path = format!("{}/chunk_{:06}", upload_dir, chunk_number);
    let mut file = tokio::fs::File::create(&chunk_path).await.map_err(|e| {
        tracing::error!("Failed to create chunk file: {:?}", e);
        AppError::Internal("Failed to store chunk".to_string())
    })?;

    file.write_all(&body).await.map_err(|e| {
        tracing::error!("Failed to write chunk: {:?}", e);
        AppError::Internal("Failed to store chunk".to_string())
    })?;

    Ok(StatusCode::ACCEPTED)
}

/// Complete chunked upload - assemble chunks and create media record
/// POST /api/media/chunked/:upload_id/complete
pub async fn complete_chunked_upload(
    State((pool, settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(upload_id): Path<String>,
) -> Result<Json<MediaUploadResponse>, AppError> {
    let base = get_storage_base(&pool).await;
    let upload_dir = format!("{}/chunks/{}", base, upload_id);

    // Read metadata
    let metadata_path = format!("{}/metadata.json", upload_dir);
    let metadata_str = tokio::fs::read_to_string(&metadata_path)
        .await
        .map_err(|_| AppError::NotFound("Upload not found".to_string()))?;

    let metadata: serde_json::Value = serde_json::from_str(&metadata_str)
        .map_err(|_| AppError::Internal("Corrupt upload metadata".to_string()))?;

    if metadata["owner_id"].as_str() != Some(&auth.user.user_id) {
        return Err(AppError::Forbidden);
    }

    let total_chunks = metadata["total_chunks"].as_i64().unwrap_or(0);
    let filename = metadata["filename"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    let mime_type = metadata["mime_type"]
        .as_str()
        .unwrap_or("application/octet-stream")
        .to_string();
    let media_type_str = metadata["media_type"]
        .as_str()
        .unwrap_or("file")
        .to_string();

    // Verify all chunks are present
    for i in 0..total_chunks {
        let chunk_path = format!("{}/chunk_{:06}", upload_dir, i);
        if !tokio::fs::try_exists(&chunk_path).await.unwrap_or(false) {
            return Err(AppError::BadRequest(format!("Missing chunk {}", i)));
        }
    }

    // Assemble chunks into final file
    let media_id = Uuid::new_v4().to_string();
    let storage_path = format!("{}/{}", base, media_id);
    tokio::fs::create_dir_all(&base).await.map_err(|e| {
        tracing::error!("Failed to create storage directory: {:?}", e);
        AppError::Internal("Failed to store file".to_string())
    })?;

    let mut output_file = tokio::fs::File::create(&storage_path).await.map_err(|e| {
        tracing::error!("Failed to create output file: {:?}", e);
        AppError::Internal("Failed to assemble file".to_string())
    })?;

    let mut hasher = Sha256::new();
    let mut total_written: i64 = 0;

    for i in 0..total_chunks {
        let chunk_path = format!("{}/chunk_{:06}", upload_dir, i);
        let chunk_data = tokio::fs::read(&chunk_path).await.map_err(|e| {
            tracing::error!("Failed to read chunk {}: {:?}", i, e);
            AppError::Internal("Failed to assemble file".to_string())
        })?;

        hasher.update(&chunk_data);
        total_written += chunk_data.len() as i64;

        output_file.write_all(&chunk_data).await.map_err(|e| {
            tracing::error!("Failed to write assembled chunk: {:?}", e);
            AppError::Internal("Failed to assemble file".to_string())
        })?;
    }

    let file_hash = hex::encode(hasher.finalize());

    // Generate thumbnail and get dimensions BEFORE encrypting
    let thumbnail_dir = format!("{}/thumbnails", base);
    let thumbnail_path = match media_type_str.as_str() {
        "image" => {
            thumbnail::generate_image_thumbnail(&storage_path, &thumbnail_dir, &media_id).await
        }
        "video" => {
            thumbnail::generate_video_thumbnail(&storage_path, &thumbnail_dir, &media_id).await
        }
        _ => None,
    };

    let (width, height) = if media_type_str == "image" {
        let sp = storage_path.clone();
        tokio::task::spawn_blocking(move || image::image_dimensions(&sp).ok())
            .await
            .ok()
            .flatten()
            .map(|(w, h)| (Some(w as i32), Some(h as i32)))
            .unwrap_or((None, None))
    } else {
        (None, None)
    };

    // Optionally encrypt the file at rest
    let encryption_nonce = encrypt_file_on_disk(&storage_path, &settings, &pool).await?;

    let now = Utc::now().to_rfc3339();
    let repo = MediaRepository::new(pool);

    // Check for duplicate by hash
    if let Some(existing) = repo
        .get_by_hash(&auth.user.user_id, &file_hash)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check for duplicate: {:?}", e);
            AppError::Internal("Failed to check for duplicate".to_string())
        })?
    {
        // Clean up assembled file and chunks
        let _ = tokio::fs::remove_file(&storage_path).await;
        let _ = tokio::fs::remove_dir_all(&upload_dir).await;

        return Ok(Json(MediaUploadResponse {
            media_id: existing.media_id.clone(),
            url: format!("/api/media/{}/file", existing.media_id),
            thumbnail_url: Some(format!("/api/media/{}/thumbnail", existing.media_id)),
        }));
    }

    let media = Media {
        media_id: media_id.clone(),
        owner_id: auth.user.user_id,
        media_type: media_type_str,
        storage_path,
        thumbnail_path,
        filename,
        mime_type,
        file_size: total_written,
        duration_seconds: None,
        width,
        height,
        file_hash,
        description: None,
        tags: None,
        encryption_nonce,
        is_deleted: 0,
        origin_server: None,
        origin_media_id: None,
        is_cached: 1,
        is_fetching: 0,
        created_at: now.clone(),
        updated_at: now,
        proxy_owner_id: None,
    };

    repo.create(&media).await.map_err(|e| {
        tracing::error!("Failed to create media record: {:?}", e);
        AppError::Internal("Failed to store media".to_string())
    })?;

    // Clean up chunk directory
    let _ = tokio::fs::remove_dir_all(&upload_dir).await;

    Ok(Json(MediaUploadResponse {
        media_id: media_id.clone(),
        url: format!("/api/media/{}/file", media_id),
        thumbnail_url: Some(format!("/api/media/{}/thumbnail", media_id)),
    }))
}

/// Get a single media item
/// GET /api/media/:media_id
pub async fn get_media(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(media_id): Path<String>,
) -> Result<Json<MediaItemResponse>, AppError> {
    let repo = MediaRepository::new(pool);

    let media = repo.get_by_id(&media_id).await.map_err(|e| {
        tracing::error!("Failed to fetch media: {:?}", e);
        AppError::Internal("Failed to fetch media".to_string())
    })?;

    let media = media.ok_or(AppError::NotFound("Not found".to_string()))?;

    // Verify ownership
    if media.owner_id != auth.user.user_id {
        return Err(AppError::Forbidden);
    }

    let filename = media.filename.clone();
    let description = media.description.clone();
    let tags: Vec<String> = media
        .tags
        .map(|t| serde_json::from_str(&t).unwrap_or_default())
        .unwrap_or_default();

    Ok(Json(MediaItemResponse {
        media_id: media.media_id.clone(),
        media_type: media.media_type,
        filename,
        mime_type: media.mime_type,
        file_size: media.file_size,
        url: format!("/api/media/{}/file", media.media_id),
        thumbnail_url: Some(format!("/api/media/{}/thumbnail", media.media_id)),
        duration_seconds: media.duration_seconds,
        width: media.width,
        height: media.height,
        description,
        tags,
        created_at: media.created_at,
        updated_at: media.updated_at,
    }))
}

/// Fetch an uncached federated media stub from its origin server and cache it locally.
/// Extracted into its own fn so its large future doesn't inflate `serve_media_file`.
async fn fetch_and_cache_federated_media(
    pool: AnyPool,
    settings: Arc<Settings>,
    s2s_client: crate::federation::client::S2SClient,
    server_identity: Arc<crate::federation::identity::ServerIdentity>,
    media: magnolia_common::models::Media,
    requesting_user_id: String,
) -> Result<magnolia_common::models::Media, AppError> {
    let (origin_server, origin_media_id) = match (&media.origin_server, &media.origin_media_id) {
        (Some(s), Some(i)) => (s.clone(), i.clone()),
        _ => return Err(AppError::NotFound("Media not available".to_string())),
    };
    let repo = MediaRepository::new(pool.clone());
    let claimed = repo.claim_fetching(&media.media_id).await.map_err(|e| {
        tracing::error!("claim_fetching failed: {:?}", e);
        AppError::Internal("Failed to claim fetch lock".to_string())
    })?;
    if !claimed {
        return Err(AppError::BadRequest("Media fetch in progress".to_string()));
    }
    let our_address = settings.base_url.trim_end_matches('/').to_string();
    let result = crate::federation::client::fetch_media(
        &s2s_client,
        &server_identity,
        &our_address,
        &origin_server,
        &origin_media_id,
        &requesting_user_id,
    )
    .await;
    match result {
        Ok((bytes, _)) => {
            let base = get_storage_base(&pool).await;
            let fed_dir = format!("{}/fed", base);
            let _ = tokio::fs::create_dir_all(&fed_dir).await;
            let storage_path = format!("{}/{}", fed_dir, media.media_id);
            if let Err(e) = tokio::fs::write(&storage_path, &bytes).await {
                let _ = repo.clear_fetching(&media.media_id).await;
                return Err(AppError::Internal(format!(
                    "Failed to write federated media: {}",
                    e
                )));
            }
            let file_hash = hex::encode(Sha256::digest(&bytes));
            let now_str = chrono::Utc::now().to_rfc3339();
            let _ = repo
                .mark_cached(
                    &media.media_id,
                    &storage_path,
                    None,
                    &file_hash,
                    None,
                    &now_str,
                )
                .await;
            repo.get_by_id(&media.media_id)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?
                .ok_or_else(|| AppError::NotFound("Not found".to_string()))
        }
        Err(e) => {
            let _ = repo.clear_fetching(&media.media_id).await;
            tracing::warn!("Failed to fetch federated media {}: {}", media.media_id, e);
            Err(AppError::NotFound(
                "Media not available from remote server".to_string(),
            ))
        }
    }
}

/// Serve media file (decrypted)
/// GET /api/media/:media_id/file
/// Any authenticated user can view media (it may be shared in posts/messages)
pub async fn serve_media_file(
    State((pool, settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    axum::Extension(s2s_client): axum::Extension<crate::federation::client::S2SClient>,
    axum::Extension(server_identity): axum::Extension<
        Arc<crate::federation::identity::ServerIdentity>,
    >,
    Path(media_id): Path<String>,
) -> Result<Response, AppError> {
    let repo = MediaRepository::new(pool.clone());

    let media = repo
        .get_by_id(&media_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch media: {:?}", e);
            AppError::Internal("Failed to fetch media".to_string())
        })?
        .ok_or(AppError::NotFound("Not found".to_string()))?;

    // Block check for local (non-federated) media.
    if media.owner_id != "__fed__" {
        match repo
            .is_blocked_local(&media.owner_id, &auth.user.user_id)
            .await
        {
            Ok(true) => return Err(AppError::Forbidden),
            Ok(false) => {}
            Err(e) => {
                tracing::error!("Block check failed: {:?}", e);
                return Err(AppError::Internal("Failed to check access".to_string()));
            }
        }
    }

    // Lazy-fetch uncached federated stubs, runs in its own task to keep this future small.
    let media = if media.is_cached == 0 {
        match tokio::spawn(fetch_and_cache_federated_media(
            pool.clone(),
            settings.clone(),
            s2s_client,
            server_identity,
            media,
            auth.user.user_id.clone(),
        ))
        .await
        {
            Ok(Ok(m)) => m,
            Ok(Err(AppError::BadRequest(_))) => {
                // claim_fetching returned false, another fetch is in progress.
                return Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .header("Retry-After", "2")
                    .body(Body::from("Media fetch in progress"))
                    .map_err(|_| AppError::Internal("Response build failed".to_string()));
            }
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(AppError::Internal("Fetch task panicked".to_string())),
        }
    } else {
        media
    };

    let file_path = media.storage_path.clone();
    let file_data = tokio::fs::read(&file_path).await.map_err(|e| {
        tracing::error!("Failed to read file: {:?}", e);
        AppError::Internal("Failed to read file".to_string())
    })?;

    let file_data = if let Some(ref nonce_hex) = media.encryption_nonce {
        decrypt_data(file_data, nonce_hex, &settings)?
    } else {
        file_data
    };

    let disposition = if media.media_type == "image" || media.media_type == "video" {
        format!(
            "inline; filename=\"{}\"",
            media.filename.replace('"', "\\\"")
        )
    } else {
        format!(
            "attachment; filename=\"{}\"",
            media.filename.replace('"', "\\\"")
        )
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, media.mime_type)
        .header(header::CONTENT_LENGTH, file_data.len())
        .header(header::CONTENT_DISPOSITION, disposition)
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .body(Body::from(file_data))
        .map_err(|e| {
            tracing::error!("Failed to build response: {:?}", e);
            AppError::Internal("Failed to serve file".to_string())
        })
}

/// Serve media thumbnail
/// GET /api/media/:media_id/thumbnail
/// Any authenticated user can view thumbnails (media may be shared in posts/messages)
pub async fn serve_thumbnail(
    State((pool, settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(media_id): Path<String>,
) -> Result<Response, AppError> {
    let repo = MediaRepository::new(pool.clone());

    let media = repo.get_by_id(&media_id).await.map_err(|e| {
        tracing::error!("Failed to fetch media: {:?}", e);
        AppError::Internal("Failed to fetch media".to_string())
    })?;

    let media = media.ok_or(AppError::NotFound("Not found".to_string()))?;

    // Block check for local media.
    if media.owner_id != "__fed__" {
        match repo
            .is_blocked_local(&media.owner_id, &auth.user.user_id)
            .await
        {
            Ok(true) => return Err(AppError::Forbidden),
            Ok(false) => {}
            Err(e) => {
                tracing::error!("Block check failed: {:?}", e);
                return Err(AppError::Internal("Failed to check access".to_string()));
            }
        }
    }

    // Federated stubs with no thumbnail yet, just 404; client should use the file URL.
    if media.is_cached == 0 {
        return Err(AppError::NotFound("Thumbnail not yet cached".to_string()));
    }

    // Serve actual thumbnail if available, otherwise fall back to original for images
    // Note: thumbnails are never encrypted, but the original may be
    let (thumb_data, content_type) = if let Some(ref thumb_path) = media.thumbnail_path {
        match tokio::fs::read(thumb_path).await {
            Ok(data) => (data, "image/jpeg".to_string()),
            Err(_) if media.media_type == "image" => {
                // Thumbnail file missing, fall back to original (may need decryption)
                let data = tokio::fs::read(&media.storage_path).await.map_err(|e| {
                    tracing::error!("Failed to read file: {:?}", e);
                    AppError::Internal("Failed to read file".to_string())
                })?;
                let data = if let Some(ref nonce_hex) = media.encryption_nonce {
                    decrypt_data(data, nonce_hex, &settings)?
                } else {
                    data
                };
                (data, media.mime_type)
            }
            Err(_) => return Err(AppError::NotFound("No thumbnail available".to_string())),
        }
    } else if media.media_type == "image" {
        // No thumbnail generated yet, serve original (may need decryption)
        let data = tokio::fs::read(&media.storage_path).await.map_err(|e| {
            tracing::error!("Failed to read file: {:?}", e);
            AppError::Internal("Failed to read file".to_string())
        })?;
        let data = if let Some(ref nonce_hex) = media.encryption_nonce {
            decrypt_data(data, nonce_hex, &settings)?
        } else {
            data
        };
        (data, media.mime_type)
    } else {
        return Err(AppError::NotFound("No thumbnail available".to_string()));
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .body(Body::from(thumb_data))
        .map_err(|e| {
            tracing::error!("Failed to build response: {:?}", e);
            AppError::Internal("Failed to serve thumbnail".to_string())
        })?;

    Ok(response)
}

/// List media (gallery)
/// GET /api/media
pub async fn list_media(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Query(params): Query<ListMediaQuery>,
) -> Result<Json<MediaListResponse>, AppError> {
    let owner_id = auth.user.user_id;

    let repo = MediaRepository::new(pool);

    let filter = MediaFilter {
        media_type: params.media_type,
        tags: params
            .tags
            .map(|t| t.split(',').map(|s| s.trim().to_string()).collect()),
        mime_type: params.mime_type,
        min_size: params.min_size,
        max_size: params.max_size,
        from_date: params.from_date,
        to_date: params.to_date,
        limit: Some(params.limit),
        offset: Some(params.offset),
    };

    let media_list = repo.list_for_user(&owner_id, &filter).await.map_err(|e| {
        tracing::error!("Failed to list media: {:?}", e);
        AppError::Internal("Failed to list media".to_string())
    })?;

    let items: Vec<MediaItemResponse> = media_list
        .into_iter()
        .map(|m| {
            let filename = m.filename.clone();
            let description = m.description.clone();
            let tags: Vec<String> = m
                .tags
                .map(|t| serde_json::from_str(&t).unwrap_or_default())
                .unwrap_or_default();

            MediaItemResponse {
                media_id: m.media_id.clone(),
                media_type: m.media_type,
                filename,
                mime_type: m.mime_type,
                file_size: m.file_size,
                url: format!("/api/media/{}/file", m.media_id),
                thumbnail_url: Some(format!("/api/media/{}/thumbnail", m.media_id)),
                duration_seconds: m.duration_seconds,
                width: m.width,
                height: m.height,
                description,
                tags,
                created_at: m.created_at,
                updated_at: m.updated_at,
            }
        })
        .collect();

    let total = items.len() as i64;
    let has_more = items.len() as i32 >= params.limit;

    Ok(Json(MediaListResponse {
        items,
        total,
        has_more,
    }))
}

/// List images (image gallery)
/// GET /api/media/images
pub async fn list_images(
    State((pool, settings)): State<AppState>,
    auth: axum::Extension<AuthMiddleware>,
    Query(mut params): Query<ListMediaQuery>,
) -> Result<Json<MediaListResponse>, AppError> {
    params.media_type = Some("image".to_string());
    list_media(State((pool, settings)), auth, Query(params)).await
}

/// List videos (video gallery)
/// GET /api/media/videos
pub async fn list_videos(
    State((pool, settings)): State<AppState>,
    auth: axum::Extension<AuthMiddleware>,
    Query(mut params): Query<ListMediaQuery>,
) -> Result<Json<MediaListResponse>, AppError> {
    params.media_type = Some("video".to_string());
    list_media(State((pool, settings)), auth, Query(params)).await
}

/// List files (file repository)
/// GET /api/media/files
pub async fn list_files(
    State((pool, settings)): State<AppState>,
    auth: axum::Extension<AuthMiddleware>,
    Query(mut params): Query<ListMediaQuery>,
) -> Result<Json<MediaListResponse>, AppError> {
    params.media_type = Some("file".to_string());
    list_media(State((pool, settings)), auth, Query(params)).await
}

/// Update media metadata
/// PUT /api/media/:media_id
pub async fn update_media(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(media_id): Path<String>,
    Json(payload): Json<UpdateMediaRequest>,
) -> Result<Json<MediaItemResponse>, AppError> {
    payload.validate().map_err(AppError::from)?;

    let repo = MediaRepository::new(pool);
    let now = Utc::now().to_rfc3339();

    // Fetch existing media
    let media = repo.get_by_id(&media_id).await.map_err(|e| {
        tracing::error!("Failed to fetch media: {:?}", e);
        AppError::Internal("Failed to fetch media".to_string())
    })?;

    let media = media.ok_or(AppError::NotFound("Not found".to_string()))?;

    // Verify ownership
    if media.owner_id != auth.user.user_id {
        return Err(AppError::Forbidden);
    }

    // Update metadata
    let tags = payload
        .tags
        .map(|t| serde_json::to_string(&t).unwrap_or_default());

    repo.update_metadata(
        &media_id,
        payload.description.as_deref(),
        tags.as_deref(),
        &now,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to update media: {:?}", e);
        AppError::Internal("Failed to update media".to_string())
    })?;

    // Fetch updated media
    let updated = repo.get_by_id(&media_id).await.map_err(|e| {
        tracing::error!("Failed to fetch updated media: {:?}", e);
        AppError::Internal("Failed to fetch updated media".to_string())
    })?;

    let updated = updated.ok_or(AppError::NotFound("Not found".to_string()))?;

    let filename = updated.filename.clone();
    let description = updated.description.clone();
    let tags: Vec<String> = updated
        .tags
        .map(|t| serde_json::from_str(&t).unwrap_or_default())
        .unwrap_or_default();

    Ok(Json(MediaItemResponse {
        media_id: updated.media_id.clone(),
        media_type: updated.media_type,
        filename,
        mime_type: updated.mime_type,
        file_size: updated.file_size,
        url: format!("/api/media/{}/file", updated.media_id),
        thumbnail_url: Some(format!("/api/media/{}/thumbnail", updated.media_id)),
        duration_seconds: updated.duration_seconds,
        width: updated.width,
        height: updated.height,
        description,
        tags,
        created_at: updated.created_at,
        updated_at: updated.updated_at,
    }))
}

/// Delete a media item
/// DELETE /api/media/:media_id
pub async fn delete_media(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(media_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let repo = MediaRepository::new(pool);
    let now = Utc::now().to_rfc3339();

    // Fetch existing media
    let media = repo.get_by_id(&media_id).await.map_err(|e| {
        tracing::error!("Failed to fetch media: {:?}", e);
        AppError::Internal("Failed to fetch media".to_string())
    })?;

    let media = media.ok_or(AppError::NotFound("Not found".to_string()))?;

    // Verify ownership
    if media.owner_id != auth.user.user_id {
        return Err(AppError::Forbidden);
    }

    // Soft delete
    repo.soft_delete(&media_id, &now).await.map_err(|e| {
        tracing::error!("Failed to delete media: {:?}", e);
        AppError::Internal("Failed to delete media".to_string())
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Batch delete media items
/// POST /api/media/batch-delete
pub async fn batch_delete_media(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Json(payload): Json<BatchDeleteMediaRequest>,
) -> Result<Json<BatchOperationResponse>, AppError> {
    payload.validate().map_err(AppError::from)?;

    let repo = MediaRepository::new(pool);
    let now = Utc::now().to_rfc3339();

    // Verify ownership of all items before deleting
    for media_id in &payload.media_ids {
        let media = repo.get_by_id(media_id).await.map_err(|e| {
            tracing::error!("Failed to fetch media: {:?}", e);
            AppError::Internal("Failed to fetch media".to_string())
        })?;

        if let Some(m) = media {
            if m.owner_id != auth.user.user_id {
                return Err(AppError::Forbidden);
            }
        }
    }

    let deleted = repo
        .batch_soft_delete(&payload.media_ids, &now)
        .await
        .map_err(|e| {
            tracing::error!("Failed to batch delete media: {:?}", e);
            AppError::Internal("Failed to delete media".to_string())
        })?;

    let failed_ids: Vec<String> = payload
        .media_ids
        .iter()
        .skip(deleted as usize)
        .cloned()
        .collect();

    Ok(Json(BatchOperationResponse {
        success_count: deleted as i32,
        failed_ids,
    }))
}

/// Get storage usage
/// GET /api/media/storage
pub async fn get_storage_usage(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
) -> Result<Json<StorageUsageResponse>, AppError> {
    let owner_id = auth.user.user_id;

    let repo = MediaRepository::new(pool);

    let total_bytes = repo.get_storage_used(&owner_id).await.map_err(|e| {
        tracing::error!("Failed to get storage usage: {:?}", e);
        AppError::Internal("Failed to get storage usage".to_string())
    })?;

    let image_bytes = repo
        .get_storage_by_type(&owner_id, "image")
        .await
        .unwrap_or(0);
    let video_bytes = repo
        .get_storage_by_type(&owner_id, "video")
        .await
        .unwrap_or(0);
    let file_bytes = repo
        .get_storage_by_type(&owner_id, "file")
        .await
        .unwrap_or(0);

    let images = repo.count_by_type(&owner_id, "image").await.unwrap_or(0);
    let videos = repo.count_by_type(&owner_id, "video").await.unwrap_or(0);
    let files = repo.count_by_type(&owner_id, "file").await.unwrap_or(0);

    Ok(Json(StorageUsageResponse {
        total_bytes,
        image_bytes,
        video_bytes,
        file_bytes,
        item_counts: StorageItemCounts {
            images,
            videos,
            files,
            total: images + videos + files,
        },
    }))
}
