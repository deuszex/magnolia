//! Post handlers for creating, reading, updating, and deleting posts

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
use crate::middleware::auth::{AuthMiddleware, OptionalAuth};
use crate::utils::encryption::{decrypt_text, encrypt_text};
use magnolia_common::models::{Post, PostContent};
use magnolia_common::repositories::post_repository::PostSearchParams;
use magnolia_common::repositories::{
    PostRepository, PostTagRepository, SiteConfigRepository, UserRepository,
};
use magnolia_common::schemas::{
    CreatePostRequest, ListPostsQuery, PostContentResponse, PostListResponse, PostResponse,
    PostSummaryResponse, SearchPostsQuery, UpdatePostRequest,
};
use magnolia_common::{errors::AppError, repositories::CommentRepository};

type AppState = (AnyPool, Arc<Settings>);

/// Batch-fetch author display info for a list of posts
async fn resolve_author_info(
    pool: &AnyPool,
    author_ids: &[String],
) -> std::collections::HashMap<String, (Option<String>, Option<String>)> {
    let user_repo = UserRepository::new(pool.clone());
    let mut map = std::collections::HashMap::new();
    for id in author_ids {
        if map.contains_key(id) {
            continue;
        }
        if let Ok(Some(u)) = user_repo.find_by_id(id).await {
            let avatar_url = u
                .avatar_media_id
                .as_ref()
                .map(|mid| format!("/api/media/{}/thumbnail", mid));
            map.insert(id.clone(), (u.display_name, avatar_url));
        }
    }
    map
}

/// Build a thumbnail_url for media content items.
/// For image content, the `content` field stores the media_id.
fn content_thumbnail_url(content_type: &str, content: &str) -> Option<String> {
    match content_type {
        "image" => Some(format!("/api/media/{}/thumbnail", content)),
        "video" | "file" => Some(format!("/api/media/{}/file", content)),
        _ => None,
    }
}

/// Create a new post
/// POST /api/posts
pub async fn create_post(
    State((pool, settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Json(payload): Json<CreatePostRequest>,
) -> Result<Json<PostResponse>, AppError> {
    // Validate request
    payload.validate().map_err(AppError::from)?;

    // Check allowed content types
    let config_repo = SiteConfigRepository::new(pool.clone());
    if let Ok(site_config) = config_repo.get().await {
        for content_req in &payload.contents {
            if !site_config.is_content_type_allowed(&content_req.content_type) {
                return Err(AppError::BadRequest(format!(
                    "Content type '{}' is not enabled on this site",
                    content_req.content_type
                )));
            }
        }
    }

    let author_id = auth.user.user_id;
    let tags = payload.tags.clone();

    let repo = PostRepository::new(pool.clone());
    let tag_repo = PostTagRepository::new(pool.clone());
    let now = Utc::now().to_rfc3339();
    let post_id = Uuid::new_v4().to_string();

    // Create post
    let post = Post {
        post_id: post_id.clone(),
        author_id: author_id.clone(),
        is_published: if payload.publish { 1 } else { 0 },
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    repo.create(&post).await.map_err(|e| {
        tracing::error!("Failed to create post: {:?}", e);
        AppError::Internal("Failed to create post".to_string())
    })?;

    // Create post contents
    let mut content_responses = Vec::new();
    for content_req in payload.contents {
        let content_id = Uuid::new_v4().to_string();

        // Encrypt text content at rest; media content stores a media_id (no encryption needed)
        let (stored_content, content_nonce) = if content_req.content_type == "text" {
            encrypt_text(&settings, &pool, &content_req.content).await?
        } else {
            (content_req.content.clone(), None)
        };

        let content = PostContent {
            content_id: content_id.clone(),
            post_id: post_id.clone(),
            content_type: content_req.content_type.clone(),
            display_order: content_req.display_order,
            content: stored_content,
            thumbnail_path: None,
            original_filename: content_req.filename.clone(),
            mime_type: content_req.mime_type.clone(),
            file_size: None,
            created_at: now.clone(),
            content_nonce,
        };

        repo.add_content(&content).await.map_err(|e| {
            tracing::error!("Failed to add post content: {:?}", e);
            AppError::Internal("Failed to add post content".to_string())
        })?;

        let thumb = content_thumbnail_url(&content_req.content_type, &content_req.content);
        content_responses.push(PostContentResponse {
            content_id,
            content_type: content_req.content_type,
            display_order: content_req.display_order,
            content: content_req.content, // return plaintext to caller
            thumbnail_url: thumb,
            filename: content_req.filename,
            mime_type: content_req.mime_type,
            file_size: None,
        });
    }

    // Save tags
    tag_repo.set_tags(&post_id, &tags).await.map_err(|e| {
        tracing::error!("Failed to set post tags: {:?}", e);
        AppError::Internal("Failed to set post tags".to_string())
    })?;
    let saved_tags = tag_repo.get_tags(&post_id).await.unwrap_or_default();

    // Broadcast to federated peers if published.
    if payload.publish {
        use crate::federation::{
            hub,
            models::{FederatedPostContent, FederatedPostEntry},
        };
        use sha2::{Digest, Sha256};
        let base = settings.base_url.trim_end_matches('/').to_string();
        let fed_contents: Vec<FederatedPostContent> = content_responses
            .iter()
            .map(|c| {
                let (content, media_ref) = if c.content_type == "text" {
                    (c.content.clone(), None)
                } else {
                    // c.content is the local media_id; receiver creates a stub from it.
                    let mr = crate::federation::models::FederatedMediaRef {
                        media_id: c.content.clone(),
                        media_type: c.content_type.clone(),
                        mime_type: c.mime_type.clone().unwrap_or_default(),
                        file_size: c.file_size.unwrap_or(0),
                        filename: c.filename.clone().unwrap_or_default(),
                        width: None,
                        height: None,
                    };
                    (String::new(), Some(mr))
                };
                FederatedPostContent {
                    content_type: c.content_type.clone(),
                    content,
                    media_ref,
                    filename: c.filename.clone(),
                    mime_type: c.mime_type.clone(),
                    file_size: c.file_size,
                    display_order: c.display_order,
                }
            })
            .collect();
        let contents_json = serde_json::to_string(&fed_contents).unwrap_or_default();
        let content_hash = hex::encode(Sha256::digest(contents_json.as_bytes()));
        let entry = FederatedPostEntry {
            post_id: post_id.clone(),
            user_id: author_id.clone(),
            posted_at: now.clone(),
            content_hash,
            contents: fed_contents,
            author_name: None,
            author_avatar_url: None,
            tags: saved_tags.clone(),
            server_address: base,
        };
        hub::broadcast_new_post(&entry);
    }

    Ok(Json(PostResponse {
        post_id,
        author_id,
        contents: content_responses,
        tags: saved_tags,
        is_published: payload.publish,
        comment_count: 0,
        created_at: now.clone(),
        updated_at: now,
    }))
}

/// Get a single post by ID
/// GET /api/posts/:post_id
pub async fn get_post(
    State((pool, settings)): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(post_id): Path<String>,
) -> Result<Json<PostResponse>, AppError> {
    let repo = PostRepository::new(pool.clone());
    let tag_repo = PostTagRepository::new(pool);

    let post_with_content = repo.get_with_contents(&post_id).await.map_err(|e| {
        tracing::error!("Failed to fetch post: {:?}", e);
        AppError::Internal("Failed to fetch post".to_string())
    })?;

    let post_data = post_with_content.ok_or(AppError::NotFound("Not found".to_string()))?;

    // Unpublished posts are only visible to the author
    if post_data.post.is_published == 0 {
        let is_owner = auth
            .as_ref()
            .map(|a| a.user.user_id == post_data.post.author_id)
            .unwrap_or(false);
        if !is_owner {
            return Err(AppError::NotFound("Not found".to_string()));
        }
    }

    let mut content_responses = Vec::new();
    for c in post_data.contents {
        let plaintext = if c.content_type == "text" {
            decrypt_text(&settings, &c.content, &c.content_nonce)?
        } else {
            c.content.clone()
        };
        let thumb = content_thumbnail_url(&c.content_type, &c.content);
        content_responses.push(PostContentResponse {
            content_id: c.content_id,
            content_type: c.content_type,
            display_order: c.display_order,
            content: plaintext,
            thumbnail_url: thumb,
            filename: c.original_filename,
            mime_type: c.mime_type,
            file_size: c.file_size,
        });
    }

    let tags = tag_repo.get_tags(&post_id).await.unwrap_or_default();

    Ok(Json(PostResponse {
        post_id: post_data.post.post_id,
        author_id: post_data.post.author_id,
        contents: content_responses,
        tags,
        is_published: post_data.post.is_published != 0,
        comment_count: post_data.comment_count,
        created_at: post_data.post.created_at,
        updated_at: post_data.post.updated_at,
    }))
}

/// List posts (feed)
/// GET /api/posts
pub async fn list_posts(
    State((pool, settings)): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Query(params): Query<ListPostsQuery>,
) -> Result<Json<PostListResponse>, AppError> {
    let repo = PostRepository::new(pool.clone());
    let tag_repo = PostTagRepository::new(pool.clone());

    let limit = params.limit;
    let offset = params.offset;

    // Only allow include_drafts when viewing your own posts
    let include_drafts = if params.include_drafts {
        match (&auth, &params.author_id) {
            (Some(a), Some(author_id)) if a.user.user_id == *author_id => true,
            _ => false,
        }
    } else {
        false
    };

    // Fetch posts based on filters
    let posts = if let Some(ref author_id) = params.author_id {
        repo.list_by_author(author_id, include_drafts, limit, offset)
            .await
    } else {
        repo.list_published(limit, offset).await
    }
    .map_err(|e| {
        tracing::error!("Failed to list posts: {:?}", e);
        AppError::Internal("Failed to list posts".to_string())
    })?;

    let total = repo.count_published().await.map_err(|e| {
        tracing::error!("Failed to count posts: {:?}", e);
        AppError::Internal("Failed to count posts".to_string())
    })?;

    // Batch-fetch tags for all posts
    let post_ids: Vec<String> = posts.iter().map(|p| p.post_id.clone()).collect();
    let tags_map = tag_repo
        .get_tags_for_posts(&post_ids)
        .await
        .unwrap_or_default();

    // Batch-fetch author display info
    let author_ids: Vec<String> = posts.iter().map(|p| p.author_id.clone()).collect();
    let author_info = resolve_author_info(&pool, &author_ids).await;

    // Build summaries with all content items
    let mut post_summaries = Vec::new();
    for post in posts {
        let contents = repo
            .get_contents_for_post(&post.post_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch post contents: {:?}", e);
                AppError::Internal("Failed to fetch post contents".to_string())
            })?;

        let comment_count = repo.get_comment_count(&post.post_id).await.unwrap_or(0);
        let post_tags = tags_map.get(&post.post_id).cloned().unwrap_or_default();

        let (author_name, author_avatar_url) = author_info
            .get(&post.author_id)
            .cloned()
            .unwrap_or((None, None));

        let mut content_responses = Vec::new();
        for c in contents {
            let plaintext = if c.content_type == "text" {
                decrypt_text(&settings, &c.content, &c.content_nonce)?
            } else {
                c.content.clone()
            };
            let thumb = content_thumbnail_url(&c.content_type, &c.content);
            content_responses.push(PostContentResponse {
                content_id: c.content_id,
                content_type: c.content_type,
                display_order: c.display_order,
                content: plaintext,
                thumbnail_url: thumb,
                filename: c.original_filename,
                mime_type: c.mime_type,
                file_size: c.file_size,
            });
        }

        post_summaries.push(PostSummaryResponse {
            post_id: post.post_id,
            author_id: post.author_id,
            author_name,
            author_avatar_url,
            contents: content_responses,
            tags: post_tags,
            is_published: post.is_published != 0,
            comment_count,
            created_at: post.created_at,
            source_server: None,
        });
    }

    let has_more = (offset + limit) < total as i32;
    let next_cursor = if has_more {
        post_summaries.last().map(|p| p.post_id.clone())
    } else {
        None
    };

    Ok(Json(PostListResponse {
        posts: post_summaries,
        total,
        has_more,
        next_cursor,
    }))
}

/// Update a post
/// PUT /api/posts/:post_id
pub async fn update_post(
    State((pool, settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(post_id): Path<String>,
    Json(payload): Json<UpdatePostRequest>,
) -> Result<Json<PostResponse>, AppError> {
    // Validate request
    payload.validate().map_err(AppError::from)?;

    let repo = PostRepository::new(pool.clone());
    let tag_repo = PostTagRepository::new(pool.clone());
    let now = Utc::now().to_rfc3339();

    // Fetch existing post
    let existing = repo.get_by_id(&post_id).await.map_err(|e| {
        tracing::error!("Failed to fetch post: {:?}", e);
        AppError::Internal("Failed to fetch post".to_string())
    })?;

    let post = existing.ok_or(AppError::NotFound("Not found".to_string()))?;

    // Verify ownership
    if post.author_id != auth.user.user_id {
        return Err(AppError::Forbidden);
    }

    // Update tags if provided
    if let Some(ref new_tags) = payload.tags {
        tag_repo.set_tags(&post_id, new_tags).await.map_err(|e| {
            tracing::error!("Failed to update post tags: {:?}", e);
            AppError::Internal("Failed to update post tags".to_string())
        })?;
    }

    // Update contents if provided
    let mut content_responses = Vec::new();
    if let Some(contents) = payload.contents {
        // Delete existing contents
        repo.delete_contents(&post_id).await.map_err(|e| {
            tracing::error!("Failed to delete post contents: {:?}", e);
            AppError::Internal("Failed to delete post contents".to_string())
        })?;

        // Add new contents
        for content_req in contents {
            let content_id = Uuid::new_v4().to_string();
            let (stored_content, content_nonce) = if content_req.content_type == "text" {
                encrypt_text(&settings, &pool, &content_req.content).await?
            } else {
                (content_req.content.clone(), None)
            };
            let content = PostContent {
                content_id: content_id.clone(),
                post_id: post_id.clone(),
                content_type: content_req.content_type.clone(),
                display_order: content_req.display_order,
                content: stored_content,
                thumbnail_path: None,
                original_filename: content_req.filename.clone(),
                mime_type: content_req.mime_type.clone(),
                file_size: None,
                created_at: now.clone(),
                content_nonce,
            };

            repo.add_content(&content).await.map_err(|e| {
                tracing::error!("Failed to add post content: {:?}", e);
                AppError::Internal("Failed to add post content".to_string())
            })?;

            let thumb = content_thumbnail_url(&content_req.content_type, &content_req.content);
            content_responses.push(PostContentResponse {
                content_id,
                content_type: content_req.content_type,
                display_order: content_req.display_order,
                content: content_req.content, // return plaintext
                thumbnail_url: thumb,
                filename: content_req.filename,
                mime_type: content_req.mime_type,
                file_size: None,
            });
        }
    } else {
        // Fetch existing contents
        let contents = repo.get_contents_for_post(&post_id).await.map_err(|e| {
            tracing::error!("Failed to fetch post contents: {:?}", e);
            AppError::Internal("Failed to fetch post contents".to_string())
        })?;

        for c in contents {
            let plaintext = if c.content_type == "text" {
                decrypt_text(&settings, &c.content, &c.content_nonce)?
            } else {
                c.content.clone()
            };
            let thumb = content_thumbnail_url(&c.content_type, &c.content);
            content_responses.push(PostContentResponse {
                content_id: c.content_id,
                content_type: c.content_type,
                display_order: c.display_order,
                content: plaintext,
                thumbnail_url: thumb,
                filename: c.original_filename,
                mime_type: c.mime_type,
                file_size: c.file_size,
            });
        }
    }

    // Update publish status if provided
    let is_published = match payload.publish {
        Some(p) => {
            if p {
                1
            } else {
                0
            }
        }
        None => post.is_published,
    };
    if payload.publish.is_some() {
        repo.set_published(&post_id, is_published, &now)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update post publish status: {:?}", e);
                AppError::Internal("Failed to update post".to_string())
            })?;
    }

    let comment_count = repo.get_comment_count(&post_id).await.unwrap_or(0);
    let tags = tag_repo.get_tags(&post_id).await.unwrap_or_default();

    Ok(Json(PostResponse {
        post_id,
        author_id: post.author_id,
        contents: content_responses,
        tags,
        is_published: is_published != 0,
        comment_count,
        created_at: post.created_at,
        updated_at: now,
    }))
}

/// Delete a post
/// DELETE /api/posts/:post_id
pub async fn delete_post(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(post_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let repo = PostRepository::new(pool.clone());

    // Fetch existing post
    let existing = repo.get_by_id(&post_id).await.map_err(|e| {
        tracing::error!("Failed to fetch post: {:?}", e);
        AppError::Internal("Failed to fetch post".to_string())
    })?;

    let post = existing.ok_or(AppError::NotFound("Not found".to_string()))?;

    // Verify ownership
    if post.author_id != auth.user.user_id {
        return Err(AppError::Forbidden);
    }

    let comment_repo = CommentRepository::new(pool);
    comment_repo.delete_for_post(&post_id).await?;

    // Delete post and contents
    repo.delete(&post_id).await.map_err(|e| {
        tracing::error!("Failed to delete post: {:?}", e);
        AppError::Internal("Failed to delete post".to_string())
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Publish/unpublish a post
/// POST /api/posts/:post_id/publish
pub async fn toggle_publish(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(post_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let repo = PostRepository::new(pool);
    let now = Utc::now().to_rfc3339();

    // Fetch existing post
    let existing = repo.get_by_id(&post_id).await.map_err(|e| {
        tracing::error!("Failed to fetch post: {:?}", e);
        AppError::Internal("Failed to fetch post".to_string())
    })?;

    let post = existing.ok_or(AppError::NotFound("Not found".to_string()))?;

    // Verify ownership
    if post.author_id != auth.user.user_id {
        return Err(AppError::Forbidden);
    }

    // Toggle publish status
    let new_status = if post.is_published != 0 { 0 } else { 1 };
    repo.set_published(&post_id, new_status, &now)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update post publish status: {:?}", e);
            AppError::Internal("Failed to update post".to_string())
        })?;

    Ok(Json(serde_json::json!({
    "post_id": post_id,
    "is_published": new_status != 0
    })))
}

/// Search posts with filters
/// GET /api/posts/search
pub async fn search_posts(
    State((pool, settings)): State<AppState>,
    Query(params): Query<SearchPostsQuery>,
) -> Result<Json<PostListResponse>, AppError> {
    let repo = PostRepository::new(pool.clone());
    let tag_repo = PostTagRepository::new(pool.clone());

    let search_params = PostSearchParams {
        text_query: params.q.clone(),
        tags: params.tags.as_ref().map(|t| {
            t.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        }),
        has_images: params.has_images,
        has_videos: params.has_videos,
        has_files: params.has_files,
        from_date: params.from_date.clone(),
        to_date: params.to_date.clone(),
        author_id: params.author_id.clone(),
        limit: params.limit,
        offset: params.offset,
    };

    let posts = repo.search_posts(&search_params).await.map_err(|e| {
        tracing::error!("Failed to search posts: {:?}", e);
        AppError::Internal("Failed to search posts".to_string())
    })?;

    let total = repo
        .count_search_results(&search_params)
        .await
        .map_err(|e| {
            tracing::error!("Failed to count search results: {:?}", e);
            AppError::Internal("Failed to count search results".to_string())
        })?;

    let post_ids: Vec<String> = posts.iter().map(|p| p.post_id.clone()).collect();
    let tags_map = tag_repo
        .get_tags_for_posts(&post_ids)
        .await
        .unwrap_or_default();

    let author_ids: Vec<String> = posts.iter().map(|p| p.author_id.clone()).collect();
    let author_info = resolve_author_info(&pool, &author_ids).await;

    let mut post_summaries = Vec::new();
    for post in posts {
        let contents = repo
            .get_contents_for_post(&post.post_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch post contents: {:?}", e);
                AppError::Internal("Failed to fetch post contents".to_string())
            })?;

        let comment_count = repo.get_comment_count(&post.post_id).await.unwrap_or(0);
        let post_tags = tags_map.get(&post.post_id).cloned().unwrap_or_default();

        let (author_name, author_avatar_url) = author_info
            .get(&post.author_id)
            .cloned()
            .unwrap_or((None, None));

        let mut content_responses = Vec::new();
        for c in contents {
            let plaintext = if c.content_type == "text" {
                decrypt_text(&settings, &c.content, &c.content_nonce)?
            } else {
                c.content.clone()
            };
            let thumb = content_thumbnail_url(&c.content_type, &c.content);
            content_responses.push(PostContentResponse {
                content_id: c.content_id,
                content_type: c.content_type,
                display_order: c.display_order,
                content: plaintext,
                thumbnail_url: thumb,
                filename: c.original_filename,
                mime_type: c.mime_type,
                file_size: c.file_size,
            });
        }

        post_summaries.push(PostSummaryResponse {
            post_id: post.post_id,
            author_id: post.author_id,
            author_name,
            author_avatar_url,
            contents: content_responses,
            tags: post_tags,
            is_published: post.is_published != 0,
            comment_count,
            created_at: post.created_at,
            source_server: None,
        });
    }

    let has_more = (params.offset + params.limit) < total as i32;
    let next_cursor = if has_more {
        post_summaries.last().map(|p| p.post_id.clone())
    } else {
        None
    };

    Ok(Json(PostListResponse {
        posts: post_summaries,
        total,
        has_more,
        next_cursor,
    }))
}
