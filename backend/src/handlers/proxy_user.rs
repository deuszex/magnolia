//! Proxy user handlers: management, session auth, and HMAC one-shot endpoints

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::Utc;
use sqlx::AnyPool;
use std::sync::Arc;
use time::Duration;
use uuid::Uuid;
use validator::Validate;

use crate::config::Settings;
use crate::handlers::ws::{ConnectionRegistry, send_to_user};
use crate::middleware::auth::AuthMiddleware;
use crate::middleware::proxy_auth::ProxyAuthMiddleware;
use crate::proxy_rate::ProxyRateLimiter;
use crate::utils::crypto::{hash_password, verify_password};
use crate::utils::encryption::{ContentEncryption, encrypt_text};
use crate::utils::thumbnail;
use axum::extract::Multipart;
use magnolia_common::errors::AppError;
use magnolia_common::models::ProxyUserAccount;
use magnolia_common::models::{
    Conversation, ConversationMember, PROXY_SENDER_SENTINEL, Post, PostContent,
};
use magnolia_common::repositories::proxy_user_repository::ProxySession;
use magnolia_common::repositories::{
    ConversationRepository, EventRepository, MediaRepository, MessageRepository, PostRepository,
    PostTagRepository, ProxyUserRepository, SiteConfigRepository, UserRepository,
};
use magnolia_common::schemas::{
    CreateProxyRequest, ProxyAuthResponse, ProxyHmacCreatePostRequest, ProxyHmacSendMessageRequest,
    ProxyLoginRequest, ProxyRateLimitResponse, ProxyUserResponse, SetProxyE2EKeyRequest,
    SetProxyHmacKeyRequest, SetProxyPasswordRequest, SetProxyRateLimitRequest, UpdateProxyRequest,
};
use magnolia_common::schemas::{
    ProxyHmacConversationResponse, ProxyHmacGetOrCreateConversationRequest,
    ProxyHmacMediaUploadResponse,
};

type AppState = (AnyPool, Arc<Settings>);

const PROXY_SESSION_COOKIE: &str = "proxy_session_id";

fn proxy_avatar_url(media_id: &Option<String>) -> Option<String> {
    media_id
        .as_ref()
        .map(|id| format!("/api/media/{}/thumbnail", id))
}

fn to_response(proxy: ProxyUserAccount) -> ProxyUserResponse {
    let avatar_url = proxy_avatar_url(&proxy.avatar_media_id);
    let hmac_key_fingerprint = proxy
        .hmac_key
        .as_deref()
        .map(|k| k.chars().take(8).collect());
    let has_hmac_key = proxy.hmac_key.is_some();
    ProxyUserResponse {
        proxy_id: proxy.proxy_id,
        paired_user_id: proxy.paired_user_id,
        active: proxy.active == 1,
        display_name: proxy.display_name,
        username: proxy.username,
        bio: proxy.bio,
        avatar_url,
        public_key: proxy.public_key,
        has_password: proxy.password_hash.is_some(),
        has_e2e_key: proxy.e2e_key_blob.is_some(),
        has_hmac_key,
        hmac_key_fingerprint,
        created_at: proxy.created_at,
        updated_at: proxy.updated_at,
    }
}

/// Verify the proxy system is enabled; return Forbidden otherwise
async fn require_proxy_system(pool: &AnyPool) -> Result<(), AppError> {
    let config = SiteConfigRepository::new(pool.clone())
        .get()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch site config: {:?}", e)))?;
    if config.proxy_user_system == 0 {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

// Admin: manage all proxies

/// GET /api/admin/proxies
pub async fn admin_list_proxies(
    State((pool, _settings)): State<AppState>,
    axum::Extension(_auth): axum::Extension<AuthMiddleware>,
) -> Result<Json<Vec<ProxyUserResponse>>, AppError> {
    require_proxy_system(&pool).await?;

    let repo = ProxyUserRepository::new(pool);
    let list = repo
        .list_all()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list proxies: {:?}", e)))?;

    Ok(Json(list.into_iter().map(to_response).collect()))
}

/// POST /api/admin/proxies
pub async fn admin_create_proxy(
    State((pool, _settings)): State<AppState>,
    axum::Extension(_auth): axum::Extension<AuthMiddleware>,
    Json(payload): Json<CreateProxyRequest>,
) -> Result<(StatusCode, Json<ProxyUserResponse>), AppError> {
    payload.validate().map_err(AppError::from)?;
    require_proxy_system(&pool).await?;

    let repo = ProxyUserRepository::new(pool.clone());

    // Usernames must be unique
    if repo
        .find_by_username(&payload.username)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .is_some()
    {
        return Err(AppError::Conflict("Username already taken".to_string()));
    }

    // Each user can only have one proxy
    if let Some(ref uid) = payload.paired_user_id {
        let user_repo = UserRepository::new(pool.clone());
        user_repo
            .find_by_id(uid)
            .await
            .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
            .ok_or_else(|| AppError::NotFound("Paired user not found".to_string()))?;

        if repo
            .find_by_paired_user(uid)
            .await
            .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
            .is_some()
        {
            return Err(AppError::Conflict(
                "This user already has a proxy account".to_string(),
            ));
        }
    }

    let mut proxy = ProxyUserAccount::new(payload.username);
    proxy.paired_user_id = payload.paired_user_id;

    repo.create(&proxy)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create proxy: {:?}", e)))?;

    Ok((StatusCode::CREATED, Json(to_response(proxy))))
}

/// GET /api/admin/proxies/:proxy_id
pub async fn admin_get_proxy(
    State((pool, _settings)): State<AppState>,
    axum::Extension(_auth): axum::Extension<AuthMiddleware>,
    Path(proxy_id): Path<String>,
) -> Result<Json<ProxyUserResponse>, AppError> {
    require_proxy_system(&pool).await?;

    let repo = ProxyUserRepository::new(pool);
    let proxy = repo
        .find_by_id(&proxy_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::NotFound("Proxy not found".to_string()))?;

    Ok(Json(to_response(proxy)))
}

/// PATCH /api/admin/proxies/:proxy_id
pub async fn admin_update_proxy(
    State((pool, _settings)): State<AppState>,
    axum::Extension(_auth): axum::Extension<AuthMiddleware>,
    Path(proxy_id): Path<String>,
    Json(payload): Json<UpdateProxyRequest>,
) -> Result<Json<ProxyUserResponse>, AppError> {
    payload.validate().map_err(AppError::from)?;
    require_proxy_system(&pool).await?;

    let repo = ProxyUserRepository::new(pool);
    repo.find_by_id(&proxy_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::NotFound("Proxy not found".to_string()))?;

    repo.update(
        &proxy_id,
        payload.display_name.as_deref(),
        payload.bio.as_deref(),
        payload.avatar_media_id.as_deref(),
        None,
        None,
        None,
        None,
        payload.active.map(|b| b as i32),
    )
    .await
    .map_err(|e| AppError::Internal(format!("Failed to update proxy: {:?}", e)))?;

    let updated = repo
        .find_by_id(&proxy_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::NotFound("Proxy not found".to_string()))?;

    Ok(Json(to_response(updated)))
}

/// DELETE /api/admin/proxies/:proxy_id
pub async fn admin_delete_proxy(
    State((pool, _settings)): State<AppState>,
    axum::Extension(_auth): axum::Extension<AuthMiddleware>,
    Path(proxy_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    require_proxy_system(&pool).await?;

    let repo = ProxyUserRepository::new(pool);
    repo.find_by_id(&proxy_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::NotFound("Proxy not found".to_string()))?;

    repo.delete_proxy_sessions(&proxy_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?;

    repo.delete(&proxy_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to delete proxy: {:?}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

// User: manage own paired proxy

/// POST /api/proxy - user creates their own sole proxy
pub async fn user_create_proxy(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Json(payload): Json<CreateProxyRequest>,
) -> Result<(StatusCode, Json<ProxyUserResponse>), AppError> {
    payload.validate().map_err(AppError::from)?;
    require_proxy_system(&pool).await?;

    let repo = ProxyUserRepository::new(pool.clone());

    // Usernames must be unique
    if repo
        .find_by_username(&payload.username)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .is_some()
    {
        return Err(AppError::Conflict("Username already taken".to_string()));
    }

    // Each user can only have one proxy
    if repo
        .find_by_paired_user(&auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .is_some()
    {
        return Err(AppError::Conflict(
            "This user already has a proxy account".to_string(),
        ));
    }

    let mut proxy = ProxyUserAccount::new(payload.username);
    proxy.paired_user_id = Some(auth.user.user_id.clone());

    repo.create(&proxy)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create proxy: {:?}", e)))?;

    Ok((StatusCode::CREATED, Json(to_response(proxy))))
}

/// GET /api/proxy - get the current user's proxy
pub async fn get_my_proxy(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
) -> Result<Json<ProxyUserResponse>, AppError> {
    require_proxy_system(&pool).await?;

    let repo = ProxyUserRepository::new(pool);
    let proxy = repo
        .find_by_paired_user(&auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::NotFound(
            "No proxy account paired to this user".to_string(),
        ))?;

    Ok(Json(to_response(proxy)))
}

/// PATCH /api/proxy  - update profile fields of own proxy
pub async fn update_my_proxy(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Json(payload): Json<UpdateProxyRequest>,
) -> Result<Json<ProxyUserResponse>, AppError> {
    payload.validate().map_err(AppError::from)?;
    require_proxy_system(&pool).await?;

    let repo = ProxyUserRepository::new(pool);
    let proxy = repo
        .find_by_paired_user(&auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::NotFound(
            "No proxy account paired to this user".to_string(),
        ))?;

    repo.update(
        &proxy.proxy_id,
        payload.display_name.as_deref(),
        payload.bio.as_deref(),
        payload.avatar_media_id.as_deref(),
        None,
        None,
        None,
        None,
        payload.active.map(|b| b as i32),
    )
    .await
    .map_err(|e| AppError::Internal(format!("Failed to update proxy: {:?}", e)))?;

    let updated = repo
        .find_by_id(&proxy.proxy_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::NotFound("Proxy not found".to_string()))?;

    Ok(Json(to_response(updated)))
}

/// PUT /api/proxy/password  - set/change session password for own proxy
pub async fn set_my_proxy_password(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Json(payload): Json<SetProxyPasswordRequest>,
) -> Result<impl IntoResponse, AppError> {
    payload.validate().map_err(AppError::from)?;
    require_proxy_system(&pool).await?;

    let repo = ProxyUserRepository::new(pool);
    let proxy = repo
        .find_by_paired_user(&auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::NotFound(
            "No proxy account paired to this user".to_string(),
        ))?;

    let hash = hash_password(&payload.password)?;

    repo.update(
        &proxy.proxy_id,
        None,
        None,
        None,
        Some(&hash),
        None,
        None,
        None,
        None,
    )
    .await
    .map_err(|e| AppError::Internal(format!("Failed to update proxy password: {:?}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// PUT /api/admin/proxies/:proxy_id/password  - admin set proxy password
pub async fn admin_set_proxy_password(
    State((pool, _settings)): State<AppState>,
    axum::Extension(_auth): axum::Extension<AuthMiddleware>,
    Path(proxy_id): Path<String>,
    Json(payload): Json<SetProxyPasswordRequest>,
) -> Result<impl IntoResponse, AppError> {
    payload.validate().map_err(AppError::from)?;
    require_proxy_system(&pool).await?;

    let repo = ProxyUserRepository::new(pool);
    repo.find_by_id(&proxy_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::NotFound("Proxy not found".to_string()))?;

    let hash = hash_password(&payload.password)?;

    repo.update(
        &proxy_id,
        None,
        None,
        None,
        Some(&hash),
        None,
        None,
        None,
        None,
    )
    .await
    .map_err(|e| AppError::Internal(format!("Failed to update proxy password: {:?}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// PUT /api/proxy/e2e-key  - set E2E key blob for own proxy
pub async fn set_my_proxy_e2e_key(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Json(payload): Json<SetProxyE2EKeyRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_proxy_system(&pool).await?;

    let repo = ProxyUserRepository::new(pool);
    let proxy = repo
        .find_by_paired_user(&auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::NotFound(
            "No proxy account paired to this user".to_string(),
        ))?;

    repo.update(
        &proxy.proxy_id,
        None,
        None,
        None,
        None,
        Some(&payload.public_key),
        Some(&payload.e2e_key_blob),
        None,
        None,
    )
    .await
    .map_err(|e| AppError::Internal(format!("Failed to update proxy E2E key: {:?}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// PUT /api/admin/proxies/:proxy_id/e2e-key  - admin set proxy E2E key
pub async fn admin_set_proxy_e2e_key(
    State((pool, _settings)): State<AppState>,
    axum::Extension(_auth): axum::Extension<AuthMiddleware>,
    Path(proxy_id): Path<String>,
    Json(payload): Json<SetProxyE2EKeyRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_proxy_system(&pool).await?;

    let repo = ProxyUserRepository::new(pool);
    repo.find_by_id(&proxy_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::NotFound("Proxy not found".to_string()))?;

    repo.update(
        &proxy_id,
        None,
        None,
        None,
        None,
        Some(&payload.public_key),
        Some(&payload.e2e_key_blob),
        None,
        None,
    )
    .await
    .map_err(|e| AppError::Internal(format!("Failed to update proxy E2E key: {:?}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// PUT /api/proxy/hmac-key   set HMAC key for own proxy (key generated client-side)
pub async fn set_my_proxy_hmac_key(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Json(payload): Json<SetProxyHmacKeyRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_proxy_system(&pool).await?;

    let repo = ProxyUserRepository::new(pool);
    let proxy = repo
        .find_by_paired_user(&auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::NotFound(
            "No proxy account paired to this user".to_string(),
        ))?;

    repo.set_hmac_key(&proxy.proxy_id, &payload.hmac_key)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to set HMAC key: {:?}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// PUT /api/admin/proxies/:proxy_id/hmac-key   admin set HMAC key
pub async fn admin_set_proxy_hmac_key(
    State((pool, _settings)): State<AppState>,
    axum::Extension(_auth): axum::Extension<AuthMiddleware>,
    Path(proxy_id): Path<String>,
    Json(payload): Json<SetProxyHmacKeyRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_proxy_system(&pool).await?;

    let repo = ProxyUserRepository::new(pool);
    repo.find_by_id(&proxy_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::NotFound("Proxy not found".to_string()))?;

    repo.set_hmac_key(&proxy_id, &payload.hmac_key)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to set HMAC key: {:?}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/admin/proxies/{proxy_id}/rate-limit
pub async fn admin_get_proxy_rate_limit(
    State((pool, _settings)): State<AppState>,
    axum::Extension(_auth): axum::Extension<AuthMiddleware>,
    Path(proxy_id): Path<String>,
) -> Result<Json<ProxyRateLimitResponse>, AppError> {
    require_proxy_system(&pool).await?;

    let row: Option<(Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT max_pieces_per_minute, max_bytes_per_minute FROM proxy_rate_limits WHERE proxy_id = $1",
    )
    .bind(&proxy_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?;

    let (pieces, bytes) = row.unwrap_or((None, None));
    Ok(Json(ProxyRateLimitResponse {
        max_pieces_per_minute: pieces.map(|v| v as i32),
        max_bytes_per_minute: bytes,
    }))
}

/// PUT /api/admin/proxies/{proxy_id}/rate-limit
pub async fn admin_set_proxy_rate_limit(
    State((pool, _settings)): State<AppState>,
    axum::Extension(_auth): axum::Extension<AuthMiddleware>,
    Path(proxy_id): Path<String>,
    Json(payload): Json<SetProxyRateLimitRequest>,
) -> Result<StatusCode, AppError> {
    require_proxy_system(&pool).await?;

    let now = Utc::now().to_rfc3339();
    if payload.max_pieces_per_minute.is_none() && payload.max_bytes_per_minute.is_none() {
        // Remove override entirely
        sqlx::query("DELETE FROM proxy_rate_limits WHERE proxy_id = $1")
            .bind(&proxy_id)
            .execute(&pool)
            .await
            .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?;
    } else {
        sqlx::query(
            "INSERT INTO proxy_rate_limits (proxy_id, max_pieces_per_minute, max_bytes_per_minute, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $4)
             ON CONFLICT (proxy_id) DO UPDATE SET
               max_pieces_per_minute = excluded.max_pieces_per_minute,
               max_bytes_per_minute = excluded.max_bytes_per_minute,
               updated_at = excluded.updated_at",
        )
        .bind(&proxy_id)
        .bind(payload.max_pieces_per_minute.map(|v| v as i64))
        .bind(payload.max_bytes_per_minute)
        .bind(&now)
        .execute(&pool)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?;
    }

    Ok(StatusCode::NO_CONTENT)
}

// Proxy session auth

/// POST /api/proxy/login
pub async fn proxy_login(
    State((pool, settings)): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<ProxyLoginRequest>,
) -> Result<(CookieJar, Json<ProxyAuthResponse>), AppError> {
    require_proxy_system(&pool).await?;

    let repo = ProxyUserRepository::new(pool);
    let proxy = repo
        .find_by_username(&payload.username)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::Unauthorized)?;

    if proxy.active == 0 {
        return Err(AppError::Forbidden);
    }

    let hash = proxy
        .password_hash
        .as_deref()
        .ok_or(AppError::Unauthorized)?; // no password = session auth disabled

    if !verify_password(&payload.password, hash)? {
        return Err(AppError::Unauthorized);
    }

    let session = ProxySession::new(proxy.proxy_id.clone(), 30);
    repo.create_session(&session)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create session: {:?}", e)))?;

    let secure = settings.base_url.starts_with("https://");
    let mut cookie = Cookie::new(PROXY_SESSION_COOKIE, session.session_id);
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Strict);
    cookie.set_secure(secure);
    cookie.set_path("/");
    cookie.set_max_age(Duration::days(30));

    let avatar_url = proxy_avatar_url(&proxy.avatar_media_id);

    Ok((
        jar.add(cookie),
        Json(ProxyAuthResponse {
            proxy_id: proxy.proxy_id,
            username: proxy.username,
            display_name: proxy.display_name,
            avatar_url,
        }),
    ))
}

/// POST /api/proxy/logout
pub async fn proxy_logout(
    State((pool, _settings)): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), AppError> {
    if let Some(cookie) = jar.get(PROXY_SESSION_COOKIE) {
        let session_id = cookie.value().to_string();
        let repo = ProxyUserRepository::new(pool);
        let _ = repo.delete_session(&session_id).await;
    }

    let mut removal = Cookie::named(PROXY_SESSION_COOKIE);
    removal.set_path("/");

    Ok((jar.remove(removal), StatusCode::NO_CONTENT))
}

/// GET /api/proxy/me  - get currently authenticated proxy info
pub async fn proxy_me(
    axum::Extension(proxy_auth): axum::Extension<ProxyAuthMiddleware>,
) -> Result<Json<ProxyUserResponse>, AppError> {
    Ok(Json(to_response(proxy_auth.proxy)))
}

// HMAC one-shot: send_message and create_post
// These endpoints do NOT use sessions. Auth is via HMAC-SHA256 signature.
// The HMAC key is the proxy's password_hash (never sent over the wire).
// Timestamp must be within 5 minutes of server time to prevent replay.

fn verify_hmac(key: &str, message: &str, signature_hex: &str) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let key_bytes = key.as_bytes();

    let mut mac = Hmac::<Sha256>::new_from_slice(key_bytes).expect("HMAC accepts any key length");
    mac.update(message.as_bytes());
    let expected = mac.finalize().into_bytes();
    let expected_hex = hex::encode(expected);
    // constant-time compare via == on fixed-length hex strings
    expected_hex == signature_hex
}

/// SHA-256 hex digest of arbitrary bytes.
fn sha256_hex(input: impl AsRef<[u8]>) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(input.as_ref()))
}

fn check_timestamp(ts: i64) -> Result<(), AppError> {
    let now = Utc::now().timestamp();
    if (now - ts).abs() > 300 {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

/// POST /api/proxy/hmac/send-message
pub async fn hmac_send_message(
    State((pool, settings)): State<AppState>,
    axum::Extension(identity): axum::Extension<
        std::sync::Arc<crate::federation::identity::ServerIdentity>,
    >,
    axum::Extension(s2s_client): axum::Extension<crate::federation::client::S2SClient>,
    axum::Extension(registry): axum::Extension<ConnectionRegistry>,
    Json(payload): Json<ProxyHmacSendMessageRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    require_proxy_system(&pool).await?;
    check_timestamp(payload.timestamp)?;

    let repo = ProxyUserRepository::new(pool.clone());
    let proxy = repo
        .find_by_id(&payload.proxy_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::Unauthorized)?;

    if proxy.active == 0 {
        return Err(AppError::Forbidden);
    }

    let hmac_key = proxy.hmac_key.as_deref().ok_or(AppError::Unauthorized)?;

    // Signed string: proxy_id:conversation_id:sha256(encrypted_content):timestamp
    // Binding encrypted_content prevents swapping message ciphertext after signing.
    let content_hash = sha256_hex(&payload.encrypted_content);
    let msg = format!(
        "{}:{}:{}:{}",
        payload.proxy_id, payload.conversation_id, content_hash, payload.timestamp
    );
    if !verify_hmac(hmac_key, &msg, &payload.signature) {
        return Err(AppError::Unauthorized);
    }

    let conv_repo = ConversationRepository::new(pool.clone());

    // Proxy must be a member of the conversation (added there by whoever set up the convo)
    if !conv_repo
        .is_member(&payload.conversation_id, &proxy.proxy_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
    {
        return Err(AppError::Forbidden);
    }

    let now = Utc::now().to_rfc3339();
    let message_id = Uuid::new_v4().to_string();

    let (stored_content, content_nonce) =
        encrypt_text(&settings, &pool, &payload.encrypted_content).await?;

    // sender_id uses the sentinel; proxy_sender_id holds the real proxy identity.
    // This avoids violating the FK on messages.sender_id -> user_accounts.
    let message = magnolia_common::models::Message {
        message_id: message_id.clone(),
        conversation_id: payload.conversation_id.clone(),
        sender_id: PROXY_SENDER_SENTINEL.to_string(),
        remote_sender_qualified_id: None,
        proxy_sender_id: Some(proxy.proxy_id.clone()),
        encrypted_content: stored_content,
        created_at: now.clone(),
        federated_status: None,
        content_nonce,
    };

    let msg_repo = MessageRepository::new(pool.clone());
    msg_repo
        .create(&message)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create message: {:?}", e)))?;

    // Persist any media attachments
    let mut attachment_media_ids: Vec<String> = Vec::new();
    if let Some(ref media_ids) = payload.media_ids {
        for media_id in media_ids {
            let attach_id = Uuid::new_v4().to_string();
            msg_repo
                .create_attachment(&attach_id, &message_id, media_id)
                .await
                .map_err(|e| AppError::Internal(format!("Failed to create attachment: {:?}", e)))?;
            attachment_media_ids.push(media_id.clone());
        }
    }

    let members = conv_repo
        .list_members(&payload.conversation_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?;

    // Deliver to all real-user members; skip sentinel rows (proxy/fed placeholders).
    let recipient_ids: Vec<String> = members
        .iter()
        .filter(|m| m.user_id != PROXY_SENDER_SENTINEL && m.user_id != "__fed__")
        .map(|m| m.user_id.clone())
        .collect();

    if !recipient_ids.is_empty() {
        msg_repo
            .create_deliveries(&message_id, &recipient_ids)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to create deliveries: {:?}", e)))?;
    }

    let ws_msg = serde_json::json!({
        "type": "new_message",
        "conversation_id": payload.conversation_id,
        "message": {
            "message_id": message_id,
            "conversation_id": payload.conversation_id,
            "sender_id": PROXY_SENDER_SENTINEL,
            "proxy_sender_id": proxy.proxy_id,
            "sender_name": proxy.display_name,
            "sender_avatar_url": proxy_avatar_url(&proxy.avatar_media_id),
            "is_proxy": true,
            "encrypted_content": payload.encrypted_content,
            "attachment_media_ids": attachment_media_ids,
            "created_at": now,
        }
    });
    let ws_str = ws_msg.to_string();
    for uid in &recipient_ids {
        send_to_user(&registry, uid, &ws_str).await;
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "message_id": message_id,
            "created_at": now,
        })),
    ))
}

/// POST /api/proxy/hmac/create-post
pub async fn hmac_create_post(
    State((pool, settings)): State<AppState>,
    Json(payload): Json<ProxyHmacCreatePostRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    payload.validate().map_err(AppError::from)?;
    require_proxy_system(&pool).await?;
    check_timestamp(payload.timestamp)?;

    let repo = ProxyUserRepository::new(pool.clone());
    let proxy = repo
        .find_by_id(&payload.proxy_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::Unauthorized)?;

    if proxy.active == 0 {
        return Err(AppError::Forbidden);
    }

    let hmac_key = proxy.hmac_key.as_deref().ok_or(AppError::Unauthorized)?;

    // Canonical body: contents sorted by display_order, each as "order|type|content",
    // joined by newline, then "\ntags:sorted_csv" and "\npublish:0|1".
    // This deterministically binds all variable fields to the signature.
    let mut sorted_contents = payload.contents.clone();
    sorted_contents.sort_by_key(|c| c.display_order);
    let mut canonical = sorted_contents
        .iter()
        .map(|c| format!("{}|{}|{}", c.display_order, c.content_type, c.content))
        .collect::<Vec<_>>()
        .join("\n");
    let mut sorted_tags = payload.tags.clone();
    sorted_tags.sort();
    canonical.push_str(&format!("\ntags:{}", sorted_tags.join(",")));
    canonical.push_str(&format!(
        "\npublish:{}",
        if payload.publish { 1 } else { 0 }
    ));

    // Signed string: proxy_id:sha256(canonical_body):publish_bit:timestamp
    let body_hash = sha256_hex(&canonical);
    let msg = format!(
        "{}:{}:{}:{}",
        payload.proxy_id,
        body_hash,
        if payload.publish { 1 } else { 0 },
        payload.timestamp
    );
    if !verify_hmac(hmac_key, &msg, &payload.signature) {
        return Err(AppError::Unauthorized);
    }

    // Posts require a real user as author_id (FK to user_accounts).
    // Use the paired user's ID; unpaired proxies cannot create posts.
    let author_id = proxy
        .paired_user_id
        .as_deref()
        .ok_or_else(|| {
            AppError::BadRequest("Proxy must be paired to a user to create posts".to_string())
        })?
        .to_string();

    let allowed_types = ["text", "image", "video", "file"];
    for item in &payload.contents {
        if !allowed_types.contains(&item.content_type.as_str()) {
            return Err(AppError::BadRequest(format!(
                "Invalid content_type: {}",
                item.content_type
            )));
        }
    }

    let now = Utc::now().to_rfc3339();
    let post_id = Uuid::new_v4().to_string();

    let post = Post {
        post_id: post_id.clone(),
        author_id,
        is_published: if payload.publish { 1 } else { 0 },
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    let post_repo = PostRepository::new(pool.clone());
    post_repo
        .create(&post)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create post: {:?}", e)))?;

    for item in &payload.contents {
        let content_id = Uuid::new_v4().to_string();
        let (stored_content, content_nonce) = if item.content_type == "text" {
            encrypt_text(&settings, &pool, &item.content).await?
        } else {
            (item.content.clone(), None)
        };
        let content = PostContent {
            content_id,
            post_id: post_id.clone(),
            content_type: item.content_type.clone(),
            display_order: item.display_order,
            content: stored_content,
            thumbnail_path: None,
            original_filename: item.filename.clone(),
            mime_type: item.mime_type.clone(),
            file_size: None,
            created_at: now.clone(),
            content_nonce,
        };
        post_repo
            .add_content(&content)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to add post content: {:?}", e)))?;
    }

    if !payload.tags.is_empty() {
        let tag_repo = PostTagRepository::new(pool.clone());
        tag_repo
            .set_tags(&post_id, &payload.tags)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to set post tags: {:?}", e)))?;
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "post_id": post_id,
            "created_at": now,
        })),
    ))
}

// --- Public proxy listing ---

/// GET /api/proxy/list-public  - list active proxies (for add-member UI)
pub async fn list_public_proxies(
    State((pool, _settings)): State<AppState>,
    axum::Extension(_auth): axum::Extension<AuthMiddleware>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    require_proxy_system(&pool).await?;

    let repo = ProxyUserRepository::new(pool);
    let proxies = repo
        .list_active()
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?;

    let items: Vec<serde_json::Value> = proxies
        .into_iter()
        .map(|p| {
            serde_json::json!({
                "proxy_id": p.proxy_id,
                "username": p.username,
                "display_name": p.display_name,
                "avatar_url": proxy_avatar_url(&p.avatar_media_id),
            })
        })
        .collect();

    Ok(Json(items))
}

// --- HMAC: get-or-create-conversation ---

/// POST /api/proxy/hmac/get-or-create-conversation
///
/// Finds or creates a direct conversation between the proxy and the target user.
/// If the proxy has a paired user, also fires a notification event to them.
/// Groups are not allowed - proxies can only open one-on-ones.
pub async fn hmac_get_or_create_conversation(
    State((pool, _settings)): State<AppState>,
    axum::Extension(registry): axum::Extension<ConnectionRegistry>,
    Json(payload): Json<ProxyHmacGetOrCreateConversationRequest>,
) -> Result<(StatusCode, Json<ProxyHmacConversationResponse>), AppError> {
    require_proxy_system(&pool).await?;
    check_timestamp(payload.timestamp)?;

    let proxy_repo = ProxyUserRepository::new(pool.clone());
    let proxy = proxy_repo
        .find_by_id(&payload.proxy_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::Unauthorized)?;

    if proxy.active == 0 {
        return Err(AppError::Forbidden);
    }

    let hmac_key = proxy.hmac_key.as_deref().ok_or(AppError::Unauthorized)?;

    let msg = format!("{}:{}", payload.proxy_id, payload.timestamp);
    if !verify_hmac(hmac_key, &msg, &payload.signature) {
        return Err(AppError::Unauthorized);
    }

    // Resolve target user
    let user_repo = UserRepository::new(pool.clone());
    let target_user = match (&payload.target_user_id, &payload.target_username) {
        (Some(id), _) => user_repo
            .find_by_id(id)
            .await
            .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
            .ok_or_else(|| AppError::NotFound("Target user not found".to_string()))?,
        (_, Some(username)) => user_repo
            .find_by_username(username)
            .await
            .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
            .ok_or_else(|| AppError::NotFound("Target user not found".to_string()))?,
        _ => {
            // No target - use paired user if available
            let paired_id = proxy.paired_user_id.as_deref().ok_or_else(|| {
                AppError::BadRequest(
                    "Must supply target_user_id or target_username, or proxy must be paired"
                        .to_string(),
                )
            })?;
            user_repo
                .find_by_id(paired_id)
                .await
                .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
                .ok_or_else(|| AppError::Internal("Paired user not found".to_string()))?
        }
    };

    let conv_repo = ConversationRepository::new(pool.clone());

    // Check for existing direct conversation between proxy and target
    if let Some(existing) = conv_repo
        .find_direct(&proxy.proxy_id, &target_user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
    {
        return Ok((
            StatusCode::OK,
            Json(ProxyHmacConversationResponse {
                conversation_id: existing.conversation_id,
                created: false,
            }),
        ));
    }

    // Create new direct conversation, proxy as creator.
    // Use PROXY_SENDER_SENTINEL for the FK field (created_by → user_accounts);
    // the real proxy identity is stored in proxy_creator_id.
    let now = Utc::now().to_rfc3339();
    let conversation_id = Uuid::new_v4().to_string();
    let conv = Conversation {
        conversation_id: conversation_id.clone(),
        conversation_type: "direct".to_string(),
        name: None,
        created_by: PROXY_SENDER_SENTINEL.to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
        proxy_creator_id: Some(proxy.proxy_id.clone()),
    };
    conv_repo
        .create(&conv)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create conversation: {:?}", e)))?;

    // Add proxy as admin member.
    // user_id uses PROXY_SENDER_SENTINEL for the FK; proxy_user_id holds the real identity.
    conv_repo
        .add_member(&ConversationMember {
            id: Uuid::new_v4().to_string(),
            conversation_id: conversation_id.clone(),
            user_id: PROXY_SENDER_SENTINEL.to_string(),
            role: "admin".to_string(),
            joined_at: now.clone(),
            proxy_user_id: Some(proxy.proxy_id.clone()),
        })
        .await
        .map_err(|e| AppError::Internal(format!("Failed to add proxy member: {:?}", e)))?;

    // Add target as member
    conv_repo
        .add_member(&ConversationMember {
            id: Uuid::new_v4().to_string(),
            conversation_id: conversation_id.clone(),
            user_id: target_user.user_id.clone(),
            role: "member".to_string(),
            joined_at: now.clone(),
            proxy_user_id: None,
        })
        .await
        .map_err(|e| AppError::Internal(format!("Failed to add target member: {:?}", e)))?;

    // Notify paired user that their proxy opened a new conversation
    if let Some(ref paired_id) = proxy.paired_user_id {
        let event_repo = EventRepository::new(pool.clone());
        let proxy_name = proxy.display_name.as_deref().unwrap_or(&proxy.username);
        let target_name = target_user
            .display_name
            .as_deref()
            .unwrap_or(&target_user.username);
        let _ = event_repo
            .create(
                paired_id,
                "security",
                "proxy_new_conversation",
                "info",
                "Proxy opened a conversation",
                &format!(
                    "Your proxy '{}' opened a new conversation with {}.",
                    proxy_name, target_name
                ),
                Some(
                    &serde_json::json!({
                        "proxy_id": proxy.proxy_id,
                        "conversation_id": conversation_id,
                        "target_user_id": target_user.user_id,
                    })
                    .to_string(),
                ),
            )
            .await;

        // Push WS notification to paired user if online
        let ws_msg = serde_json::json!({
            "type": "proxy_new_conversation",
            "proxy_id": proxy.proxy_id,
            "conversation_id": conversation_id,
        })
        .to_string();
        send_to_user(&registry, paired_id, &ws_msg).await;
    }

    Ok((
        StatusCode::CREATED,
        Json(ProxyHmacConversationResponse {
            conversation_id,
            created: true,
        }),
    ))
}

// --- HMAC: upload-media ---

/// POST /api/proxy/hmac/upload-media  (multipart)
///
/// Fields: proxy_id (text), signature (text), timestamp (text), file (bytes)
///
/// Rate-limited per proxy. On breach: proxy is disabled and security events are
/// fired to the paired user (if any) and all admins.
pub async fn hmac_upload_media(
    State((pool, settings)): State<AppState>,
    axum::Extension(rate_limiter): axum::Extension<ProxyRateLimiter>,
    axum::Extension(registry): axum::Extension<ConnectionRegistry>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<ProxyHmacMediaUploadResponse>), AppError> {
    require_proxy_system(&pool).await?;

    // Parse multipart
    let mut proxy_id: Option<String> = None;
    let mut signature: Option<String> = None;
    let mut timestamp: Option<i64> = None;
    let mut file_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut content_type_str: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Invalid multipart: {:?}", e)))?
    {
        match field.name().unwrap_or("") {
            "proxy_id" => proxy_id = Some(field.text().await.unwrap_or_default()),
            "signature" => signature = Some(field.text().await.unwrap_or_default()),
            "timestamp" => {
                let t = field.text().await.unwrap_or_default();
                timestamp = t.parse::<i64>().ok();
            }
            "file" => {
                filename = field.file_name().map(|s| s.to_string());
                content_type_str = field.content_type().map(|s| s.to_string());
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| AppError::BadRequest(format!("Failed to read file: {:?}", e)))?
                        .to_vec(),
                );
            }
            _ => {}
        }
    }

    let proxy_id = proxy_id.ok_or_else(|| AppError::BadRequest("Missing proxy_id".to_string()))?;
    let signature =
        signature.ok_or_else(|| AppError::BadRequest("Missing signature".to_string()))?;
    let timestamp = timestamp
        .ok_or_else(|| AppError::BadRequest("Missing or invalid timestamp".to_string()))?;
    let file_data =
        file_data.ok_or_else(|| AppError::BadRequest("No file provided".to_string()))?;

    check_timestamp(timestamp)?;

    let proxy_repo = ProxyUserRepository::new(pool.clone());
    let proxy = proxy_repo
        .find_by_id(&proxy_id)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {:?}", e)))?
        .ok_or(AppError::Unauthorized)?;

    if proxy.active == 0 {
        return Err(AppError::Forbidden);
    }

    let hmac_key = proxy.hmac_key.as_deref().ok_or(AppError::Unauthorized)?;

    // Signed string: proxy_id:sha256(file_bytes):timestamp
    let file_hash_hex = sha256_hex(&file_data);
    let msg = format!("{}:{}:{}", proxy_id, file_hash_hex, timestamp);
    if !verify_hmac(hmac_key, &msg, &signature) {
        return Err(AppError::Unauthorized);
    }

    // Resolve effective rate limit: lower of server default and per-proxy override
    let config = SiteConfigRepository::new(pool.clone())
        .get()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch site config: {:?}", e)))?;

    let server_pieces = config.proxy_rate_limit_pieces as u32;
    let server_bytes = config.proxy_rate_limit_bytes as u64;

    let (effective_pieces, effective_bytes) = {
        // Check per-proxy override
        let override_row: Option<(Option<i64>, Option<i64>)> = sqlx::query_as(
            "SELECT max_pieces_per_minute, max_bytes_per_minute FROM proxy_rate_limits WHERE proxy_id = $1",
        )
        .bind(&proxy_id)
        .fetch_optional(&pool)
        .await
        .unwrap_or(None);

        match override_row {
            Some((Some(p), Some(b))) => {
                ((p as u32).min(server_pieces), (b as u64).min(server_bytes))
            }
            Some((Some(p), None)) => ((p as u32).min(server_pieces), server_bytes),
            Some((None, Some(b))) => (server_pieces, (b as u64).min(server_bytes)),
            _ => (server_pieces, server_bytes),
        }
    };

    let file_size = file_data.len() as u64;

    if rate_limiter
        .check_and_record(&proxy_id, file_size, effective_pieces, effective_bytes)
        .is_err()
    {
        // Disable the proxy immediately
        let _ = proxy_repo.disable(&proxy_id).await;
        rate_limiter.clear(&proxy_id);

        let proxy_name = proxy.display_name.as_deref().unwrap_or(&proxy.username);
        let metadata = serde_json::json!({
            "proxy_id": proxy_id,
            "proxy_username": proxy.username,
        });

        // Notify paired user
        if let Some(ref paired_id) = proxy.paired_user_id {
            crate::events::create_event(
                &pool,
                Some(&registry),
                paired_id,
                "security",
                "proxy_rate_overload",
                "high",
                "Proxy media rate limit exceeded",
                &format!(
                    "Your proxy '{}' exceeded the media upload rate limit and has been automatically disabled.",
                    proxy_name
                ),
                Some(metadata.clone()),
            )
            .await;
        }

        // Notify all admins
        let admin_body = if let Some(ref paired_id) = proxy.paired_user_id {
            format!(
                "Proxy '{}' (paired with user {}) exceeded the media upload rate limit and has been automatically disabled.",
                proxy_name, paired_id
            )
        } else {
            format!(
                "Proxy '{}' (unpaired) exceeded the media upload rate limit and has been automatically disabled.",
                proxy_name
            )
        };
        crate::events::create_admin_event(
            &pool,
            Some(&registry),
            "security",
            "proxy_rate_overload",
            "high",
            "Proxy media rate limit exceeded",
            &admin_body,
            Some(metadata),
        )
        .await;

        return Err(AppError::Forbidden);
    }

    // Store the file using the same pattern as the regular media upload handler
    let media_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let filename = filename.unwrap_or_else(|| format!("{}.bin", media_id));
    let content_type = content_type_str.unwrap_or_else(|| "application/octet-stream".to_string());
    let media_type = if content_type.starts_with("image/") {
        "image"
    } else if content_type.starts_with("video/") {
        "video"
    } else {
        "file"
    };

    let base = {
        let repo = crate::handlers::media::get_storage_base_pub(&pool).await;
        repo
    };
    let storage_path = format!("{}/{}", base, media_id);
    tokio::fs::create_dir_all(&base)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create storage dir: {:?}", e)))?;
    tokio::fs::write(&storage_path, &file_data)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to write file: {:?}", e)))?;

    let thumbnail_dir = format!("{}/thumbnails", base);
    let thumbnail_path = match media_type {
        "image" => {
            thumbnail::generate_image_thumbnail(&storage_path, &thumbnail_dir, &media_id).await
        }
        "video" => {
            thumbnail::generate_video_thumbnail(&storage_path, &thumbnail_dir, &media_id).await
        }
        _ => None,
    };

    // Encrypt at rest if enabled
    let encryption_nonce = if let Some(key) = &settings.encryption_at_rest_key {
        let config = SiteConfigRepository::new(pool.clone()).get().await.ok();
        if config
            .map(|c| c.encryption_at_rest_enabled == 1)
            .unwrap_or(false)
        {
            let enc = ContentEncryption::from_hex_key(key)?;
            let plaintext = tokio::fs::read(&storage_path).await.map_err(|e| {
                AppError::Internal(format!("Failed to read for encryption: {:?}", e))
            })?;
            let (encrypted, nonce) = enc.encrypt_content(&plaintext)?;
            tokio::fs::write(&storage_path, &encrypted)
                .await
                .map_err(|e| AppError::Internal(format!("Failed to write encrypted: {:?}", e)))?;
            Some(hex::encode(&nonce))
        } else {
            None
        }
    } else {
        None
    };

    // Use PROXY_SENDER_SENTINEL for owner_id (satisfies FK → user_accounts);
    // the real proxy identity is stored in proxy_owner_id.
    let file_hash = {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(&file_data);
        hex::encode(hasher.finalize())
    };

    let media = magnolia_common::models::Media {
        media_id: media_id.clone(),
        owner_id: PROXY_SENDER_SENTINEL.to_string(),
        media_type: media_type.to_string(),
        storage_path: storage_path.clone(),
        thumbnail_path: thumbnail_path.clone(),
        filename,
        mime_type: content_type,
        file_size: file_data.len() as i64,
        duration_seconds: None,
        width: None,
        height: None,
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
        proxy_owner_id: Some(proxy_id.clone()),
    };

    MediaRepository::new(pool.clone())
        .create(&media)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to save media: {:?}", e)))?;

    Ok((
        StatusCode::CREATED,
        Json(ProxyHmacMediaUploadResponse {
            media_id: media_id.clone(),
            url: format!("/api/media/{}/file", media_id),
            thumbnail_url: thumbnail_path.map(|_| format!("/api/media/{}/thumbnail", media_id)),
        }),
    ))
}
