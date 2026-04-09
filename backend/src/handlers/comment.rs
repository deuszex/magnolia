//! Comment handlers for creating, reading, updating, and deleting comments

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::Utc;
use sqlx::AnyPool;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

use crate::config::Settings;
use crate::middleware::auth::AuthMiddleware;
use magnolia_common::errors::AppError;
use magnolia_common::models::Comment;
use magnolia_common::repositories::{
    CommentRepository, MediaRepository, PostRepository, UserRepository,
};
use magnolia_common::schemas::{
    CommentCountResponse, CommentListResponse, CommentResponse, CreateCommentRequest,
    ListCommentsQuery, UpdateCommentRequest,
};

type AppState = (AnyPool, Arc<Settings>);

/// Derive media URL, media_id, and filename from a comment's content_type and content.
/// For non-text comments, content stores the media_id.
fn comment_media_fields(comment: &Comment) -> (Option<String>, Option<String>, Option<String>) {
    match comment.content_type.as_str() {
        "image" | "video" | "file" => {
            let media_id = comment.content.clone();
            let url = format!("/api/media/{}/file", media_id);
            // filename comes from the media record; we populate it async in handlers that have a MediaRepository
            (Some(url), Some(media_id), None)
        }
        _ => (None, None, None),
    }
}

/// Resolve author display name and avatar URL from user_id
async fn resolve_comment_author(pool: &AnyPool, author_id: &str) -> (String, Option<String>) {
    let user_repo = UserRepository::new(pool.clone());
    match user_repo.find_by_id(author_id).await {
        Ok(Some(u)) => {
            let name = u.display_name.unwrap_or(u.username);
            let avatar = u
                .avatar_media_id
                .map(|mid| format!("/api/media/{}/thumbnail", mid));
            (name, avatar)
        }
        _ => ("Anonymous".to_string(), None),
    }
}

async fn resolve_comment_author_cached(
    user_repo: &UserRepository,
    author_id: &str,
) -> (String, Option<String>) {
    match user_repo.find_by_id(author_id).await {
        Ok(Some(u)) => {
            let name = u.display_name.unwrap_or(u.username);
            let avatar = u
                .avatar_media_id
                .map(|mid| format!("/api/media/{}/thumbnail", mid));
            (name, avatar)
        }
        _ => ("Anonymous".to_string(), None),
    }
}

/// Create a comment on a post
/// POST /api/posts/:post_id/comments
pub async fn create_comment(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(post_id): Path<String>,
    Json(payload): Json<CreateCommentRequest>,
) -> Result<Json<CommentResponse>, AppError> {
    // Validate request
    payload.validate().map_err(AppError::from)?;

    let post_repo = PostRepository::new(pool.clone());
    let comment_repo = CommentRepository::new(pool.clone());
    let media_repo = MediaRepository::new(pool);

    // Verify post exists
    let post = post_repo.get_by_id(&post_id).await.map_err(|e| {
        tracing::error!("Failed to fetch post: {:?}", e);
        AppError::Internal("Failed to fetch post".to_string())
    })?;

    let post = post.ok_or(AppError::NotFound("Not found".to_string()))?;

    // Check if post is published (unless the commenter is the post owner)
    if post.is_published == 0 && post.author_id != auth.user.user_id {
        return Err(AppError::NotFound("Not found".to_string()));
    }

    // For media comments, validate that the media_id (stored in content) exists
    let mut media_url = None;
    let mut media_id_field = None;
    let mut filename_field = None;
    let is_media = matches!(payload.content_type.as_str(), "image" | "video" | "file");
    if is_media {
        let media = media_repo.get_by_id(&payload.content).await.map_err(|e| {
            tracing::error!("Failed to look up media: {:?}", e);
            AppError::Internal("Failed to validate media".to_string())
        })?;
        let media = media.ok_or(AppError::BadRequest("Media not found".to_string()))?;
        media_url = Some(format!("/api/media/{}/file", media.media_id));
        media_id_field = Some(media.media_id);
        filename_field = Some(media.filename);
    }

    let author_id = auth.user.user_id.clone();
    let author_display_name = auth
        .user
        .display_name
        .clone()
        .unwrap_or_else(|| auth.user.username.clone());
    let author_avatar_url = auth
        .user
        .avatar_media_id
        .as_ref()
        .map(|mid| format!("/api/media/{}/thumbnail", mid));

    let now = Utc::now().to_rfc3339();
    let comment_id = Uuid::new_v4().to_string();

    let comment = Comment {
        comment_id: comment_id.clone(),
        post_id: post_id.clone(),
        author_id: author_id.clone(),
        parent_comment_id: payload.parent_comment_id.clone(),
        content_type: payload.content_type.clone(),
        content: payload.content.clone(),
        media_path: None,
        is_deleted: 0,
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    comment_repo.create(&comment).await.map_err(|e| {
        tracing::error!("Failed to create comment: {:?}", e);
        AppError::Internal("Failed to create comment".to_string())
    })?;

    Ok(Json(CommentResponse {
        comment_id,
        post_id,
        author_id,
        author_display_name,
        author_avatar_url,
        parent_comment_id: payload.parent_comment_id,
        content_type: payload.content_type,
        content: payload.content,
        media_url,
        media_id: media_id_field,
        filename: filename_field,
        is_deleted: false,
        reply_count: 0,
        created_at: now.clone(),
        updated_at: now,
    }))
}

/// Get a single comment by ID
/// GET /api/comments/:comment_id
pub async fn get_comment(
    State((pool, _settings)): State<AppState>,
    Path(comment_id): Path<String>,
) -> Result<Json<CommentResponse>, AppError> {
    let comment_repo = CommentRepository::new(pool.clone());

    let comment = comment_repo.get_by_id(&comment_id).await.map_err(|e| {
        tracing::error!("Failed to fetch comment: {:?}", e);
        AppError::Internal("Failed to fetch comment".to_string())
    })?;

    let comment = comment.ok_or(AppError::NotFound("Not found".to_string()))?;

    if comment.is_deleted != 0 {
        return Err(AppError::NotFound("Not found".to_string()));
    }

    let reply_count = comment_repo.count_replies(&comment_id).await.unwrap_or(0);
    let (author_display_name, author_avatar_url) =
        resolve_comment_author(&pool, &comment.author_id).await;
    let (media_url, media_id, mut filename) = comment_media_fields(&comment);
    // Resolve filename from media record if this is a media comment
    if media_id.is_some() {
        let media_repo = MediaRepository::new(pool);
        if let Some(ref mid) = media_id {
            if let Ok(Some(media)) = media_repo.get_by_id(mid).await {
                filename = Some(media.filename);
            }
        }
    }

    Ok(Json(CommentResponse {
        comment_id: comment.comment_id,
        post_id: comment.post_id,
        author_id: comment.author_id,
        author_display_name,
        author_avatar_url,
        parent_comment_id: comment.parent_comment_id,
        content_type: comment.content_type,
        content: comment.content.clone(),
        media_url,
        media_id,
        filename,
        is_deleted: comment.is_deleted != 0,
        reply_count,
        created_at: comment.created_at,
        updated_at: comment.updated_at,
    }))
}

/// List comments for a post
/// GET /api/posts/:post_id/comments
pub async fn list_comments(
    State((pool, _settings)): State<AppState>,
    Path(post_id): Path<String>,
    Query(params): Query<ListCommentsQuery>,
) -> Result<Json<CommentListResponse>, AppError> {
    let comment_repo = CommentRepository::new(pool.clone());
    let post_repo = PostRepository::new(pool.clone());

    // Verify post exists
    let post = post_repo.get_by_id(&post_id).await.map_err(|e| {
        tracing::error!("Failed to fetch post: {:?}", e);
        AppError::Internal("Failed to fetch post".to_string())
    })?;

    post.ok_or(AppError::NotFound("Not found".to_string()))?;

    let limit = params.limit;
    let offset = params.offset;

    // Fetch top-level comments
    let comments = comment_repo
        .list_for_post(&post_id, limit, offset)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list comments: {:?}", e);
            AppError::Internal("Failed to list comments".to_string())
        })?;

    let total = comment_repo.count_for_post(&post_id).await.map_err(|e| {
        tracing::error!("Failed to count comments: {:?}", e);
        AppError::Internal("Failed to count comments".to_string())
    })?;

    // Batch-resolve author info
    let user_repo = UserRepository::new(pool.clone());
    let media_repo = MediaRepository::new(pool);
    let mut author_cache: std::collections::HashMap<String, (String, Option<String>)> =
        std::collections::HashMap::new();
    for c in &comments {
        if !author_cache.contains_key(&c.author_id) {
            let info = resolve_comment_author_cached(&user_repo, &c.author_id).await;
            author_cache.insert(c.author_id.clone(), info);
        }
    }

    // Build responses
    let mut comment_responses = Vec::new();
    for comment in comments {
        let reply_count = comment_repo
            .count_replies(&comment.comment_id)
            .await
            .unwrap_or(0);
        let (author_display_name, author_avatar_url) = author_cache
            .get(&comment.author_id)
            .cloned()
            .unwrap_or(("Anonymous".to_string(), None));

        let (media_url, media_id, mut filename) = comment_media_fields(&comment);
        if let Some(ref mid) = media_id {
            if let Ok(Some(media)) = media_repo.get_by_id(mid).await {
                filename = Some(media.filename);
            }
        }

        comment_responses.push(CommentResponse {
            comment_id: comment.comment_id,
            post_id: comment.post_id,
            author_id: comment.author_id,
            author_display_name,
            author_avatar_url,
            parent_comment_id: comment.parent_comment_id,
            content_type: comment.content_type,
            content: comment.content.clone(),
            media_url,
            media_id,
            filename,
            is_deleted: comment.is_deleted != 0,
            reply_count,
            created_at: comment.created_at,
            updated_at: comment.updated_at,
        });
    }

    let has_more = (offset + limit) < total as i32;

    Ok(Json(CommentListResponse {
        comments: comment_responses,
        total,
        has_more,
    }))
}

/// Get replies to a comment
/// GET /api/comments/:comment_id/replies
pub async fn list_replies(
    State((pool, _settings)): State<AppState>,
    Path(comment_id): Path<String>,
    Query(params): Query<ListCommentsQuery>,
) -> Result<Json<CommentListResponse>, AppError> {
    let comment_repo = CommentRepository::new(pool.clone());

    // Verify parent comment exists
    let parent = comment_repo.get_by_id(&comment_id).await.map_err(|e| {
        tracing::error!("Failed to fetch comment: {:?}", e);
        AppError::Internal("Failed to fetch comment".to_string())
    })?;

    parent.ok_or(AppError::NotFound("Not found".to_string()))?;

    let limit = params.limit;
    let offset = params.offset;

    // Fetch replies
    let replies = comment_repo
        .list_replies(&comment_id, limit, offset)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list replies: {:?}", e);
            AppError::Internal("Failed to list replies".to_string())
        })?;

    let total = comment_repo.count_replies(&comment_id).await.map_err(|e| {
        tracing::error!("Failed to count replies: {:?}", e);
        AppError::Internal("Failed to count replies".to_string())
    })?;

    // Batch-resolve author info
    let user_repo = UserRepository::new(pool.clone());
    let media_repo = MediaRepository::new(pool);
    let mut author_cache: std::collections::HashMap<String, (String, Option<String>)> =
        std::collections::HashMap::new();
    for r in &replies {
        if !author_cache.contains_key(&r.author_id) {
            let info = resolve_comment_author_cached(&user_repo, &r.author_id).await;
            author_cache.insert(r.author_id.clone(), info);
        }
    }

    // Build responses
    let mut reply_responses = Vec::new();
    for reply in replies {
        let nested_reply_count = comment_repo
            .count_replies(&reply.comment_id)
            .await
            .unwrap_or(0);
        let (author_display_name, author_avatar_url) = author_cache
            .get(&reply.author_id)
            .cloned()
            .unwrap_or(("Anonymous".to_string(), None));

        let (media_url, media_id, mut filename) = comment_media_fields(&reply);
        if let Some(ref mid) = media_id {
            if let Ok(Some(media)) = media_repo.get_by_id(mid).await {
                filename = Some(media.filename);
            }
        }

        reply_responses.push(CommentResponse {
            comment_id: reply.comment_id,
            post_id: reply.post_id,
            author_id: reply.author_id,
            author_display_name,
            author_avatar_url,
            parent_comment_id: reply.parent_comment_id,
            content_type: reply.content_type,
            content: reply.content.clone(),
            media_url,
            media_id,
            filename,
            is_deleted: reply.is_deleted != 0,
            reply_count: nested_reply_count,
            created_at: reply.created_at,
            updated_at: reply.updated_at,
        });
    }

    let has_more = (offset + limit) < total as i32;

    Ok(Json(CommentListResponse {
        comments: reply_responses,
        total,
        has_more,
    }))
}

/// Update a comment
/// PUT /api/comments/:comment_id
pub async fn update_comment(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(comment_id): Path<String>,
    Json(payload): Json<UpdateCommentRequest>,
) -> Result<Json<CommentResponse>, AppError> {
    // Validate request
    payload.validate().map_err(AppError::from)?;

    let comment_repo = CommentRepository::new(pool);
    let now = Utc::now().to_rfc3339();

    // Fetch existing comment
    let comment = comment_repo.get_by_id(&comment_id).await.map_err(|e| {
        tracing::error!("Failed to fetch comment: {:?}", e);
        AppError::Internal("Failed to fetch comment".to_string())
    })?;

    let comment = comment.ok_or(AppError::NotFound("Not found".to_string()))?;

    if comment.is_deleted != 0 {
        return Err(AppError::NotFound("Not found".to_string()));
    }

    // Verify ownership
    if comment.author_id != auth.user.user_id {
        return Err(AppError::Forbidden);
    }

    // Only allow text content updates
    if comment.content_type != "text" {
        return Err(AppError::BadRequest(
            "Only text comments can be edited".to_string(),
        ));
    }

    comment_repo
        .update_content(&comment_id, &payload.content, &now)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update comment: {:?}", e);
            AppError::Internal("Failed to update comment".to_string())
        })?;

    let reply_count = comment_repo.count_replies(&comment_id).await.unwrap_or(0);
    let author_display_name = auth
        .user
        .display_name
        .clone()
        .unwrap_or_else(|| auth.user.username.clone());
    let author_avatar_url = auth
        .user
        .avatar_media_id
        .as_ref()
        .map(|mid| format!("/api/media/{}/thumbnail", mid));

    Ok(Json(CommentResponse {
        comment_id,
        post_id: comment.post_id,
        author_id: comment.author_id,
        author_display_name,
        author_avatar_url,
        parent_comment_id: comment.parent_comment_id,
        content_type: comment.content_type,
        content: payload.content,
        media_url: None,
        media_id: None,
        filename: None,
        is_deleted: false,
        reply_count,
        created_at: comment.created_at,
        updated_at: now,
    }))
}

/// Delete a comment (soft delete)
/// DELETE /api/comments/:comment_id
pub async fn delete_comment(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(comment_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let comment_repo = CommentRepository::new(pool);
    let now = Utc::now().to_rfc3339();

    // Fetch existing comment
    let comment = comment_repo.get_by_id(&comment_id).await.map_err(|e| {
        tracing::error!("Failed to fetch comment: {:?}", e);
        AppError::Internal("Failed to fetch comment".to_string())
    })?;

    let comment = comment.ok_or(AppError::NotFound("Not found".to_string()))?;

    if comment.is_deleted != 0 {
        return Err(AppError::NotFound("Not found".to_string()));
    }

    // Verify ownership or admin
    if comment.author_id != auth.user.user_id && auth.user.admin == 0 {
        return Err(AppError::Forbidden);
    }

    // Soft delete
    comment_repo
        .soft_delete(&comment_id, &now)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete comment: {:?}", e);
            AppError::Internal("Failed to delete comment".to_string())
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Get comment count for a post
/// GET /api/posts/:post_id/comments/count
pub async fn get_comment_count(
    State((pool, _settings)): State<AppState>,
    Path(post_id): Path<String>,
) -> Result<Json<CommentCountResponse>, AppError> {
    let comment_repo = CommentRepository::new(pool.clone());
    let post_repo = PostRepository::new(pool);

    // Verify post exists
    let post = post_repo.get_by_id(&post_id).await.map_err(|e| {
        tracing::error!("Failed to fetch post: {:?}", e);
        AppError::Internal("Failed to fetch post".to_string())
    })?;

    post.ok_or(AppError::NotFound("Not found".to_string()))?;

    let count = comment_repo.count_for_post(&post_id).await.map_err(|e| {
        tracing::error!("Failed to count comments: {:?}", e);
        AppError::Internal("Failed to count comments".to_string())
    })?;

    Ok(Json(CommentCountResponse { post_id, count }))
}
