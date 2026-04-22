//! Authentication handlers for user registration, login, and account management

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use sqlx::AnyPool;
use std::sync::Arc;
use time::Duration;
use validator::Validate;

use crate::middleware::security_audit::generate_fingerprint;
use crate::utils::ClientIp;
use crate::utils::crypto::{hash_password, verify_password};
use crate::utils::password::validate_password_strength;
use crate::{config::Settings, handlers::email_html::reset_email::build_reset_email};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use hmac::{Hmac, Mac};
use magnolia_common::errors::AppError;
use magnolia_common::models::{
    EmailVerification, PasswordReset, RegisterApplication, Session, UserAccount,
};
use magnolia_common::repositories::{
    EmailSettingsRepository, EventRepository, InviteRepository, MediaRepository,
    RegisterApplicationRepository, SiteConfigRepository, ThemeRepository, UserRepository,
};
use rand::RngCore;
use sha2::Sha256;

use magnolia_common::schemas::{
    ApplicationResponse, AuthConfigResponse, AuthResponse, ChangePasswordRequest,
    E2EKeyBlobResponse, LoginRequest, MessageResponse, PasswordResetKeyResponse,
    PasswordResetKeyStatus, ProfileResponse, RegisterRequest, RequestPasswordResetRequest,
    ResendVerificationRequest, ResetPasswordRequest, ResetPasswordWithKeyRequest, SessionResponse,
    SetE2EKeyBlobRequest, SubmitApplicationRequest, UpdateProfileRequest, UpdatePublicKeyRequest,
    UserListItem, UserListQuery, UserListResponse, UserResponse, ValidatePasswordResetRequest,
    VerifyEmailRequest,
};

type AppState = (AnyPool, Arc<Settings>);

const SESSION_COOKIE_NAME: &str = "session_id";

fn avatar_url(media_id: &Option<String>) -> Option<String> {
    media_id
        .as_ref()
        .map(|id| format!("/api/media/{}/thumbnail", id))
}

/// Minimal HTML escaper for email bodies.
fn he(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Register a new user account
/// POST /api/auth/register
pub async fn register(
    State((pool, _settings)): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), AppError> {
    payload.validate().map_err(AppError::from)?;
    validate_password_strength(&payload.password)?;

    // Enforce registration mode
    let config_repo = SiteConfigRepository::new(pool.clone());
    let config = config_repo.get().await.map_err(|e| {
        tracing::error!("Failed to fetch site config: {:?}", e);
        AppError::Internal("Failed to fetch site configuration".to_string())
    })?;

    // Validate the invite token (if provided), a valid token bypasses mode restrictions
    let invite_opt = if let Some(ref token_str) = payload.invite_token {
        let token = token_str.trim().to_string();
        if token.is_empty() {
            None
        } else {
            let invite_repo = InviteRepository::new(pool.clone());
            let invite = invite_repo
                .find_by_token(&token)
                .await?
                .ok_or_else(|| AppError::BadRequest("Invalid invite token".to_string()))?;

            if invite.used_at.is_some() {
                return Err(AppError::BadRequest(
                    "Invite token has already been used".to_string(),
                ));
            }
            if invite.is_expired() {
                return Err(AppError::BadRequest("Invite token has expired".to_string()));
            }
            // Enforce email match when the setting is enabled and the invite has a bound address
            if config.enforce_invite_email == 1 {
                if let Some(ref bound_email) = invite.email {
                    if bound_email.to_lowercase()
                        != payload.email.as_deref().unwrap_or("").to_lowercase()
                    {
                        return Err(AppError::BadRequest(
                            "This invite is for a different email address".to_string(),
                        ));
                    }
                }
            }
            Some(invite)
        }
    } else {
        None
    };

    // Apply registration mode restrictions, a valid invite grants access in any mode
    match config.registration_mode.as_str() {
        "application" if invite_opt.is_none() => {
            return Err(AppError::Forbidden);
        }
        "invite_only" if invite_opt.is_none() => {
            return Err(AppError::BadRequest(
                "An invite token is required to register".to_string(),
            ));
        }
        _ => {} // open, or any mode with a valid invite
    }

    let user_repo = UserRepository::new(pool.clone());

    // Validate email uniqueness if provided.
    if let Some(ref email) = payload.email {
        if !email.is_empty() {
            if user_repo.find_by_email(email).await?.is_some() {
                return Err(AppError::Conflict("Email already registered".to_string()));
            }
        }
    }

    // Validate username uniqueness.
    if user_repo
        .find_by_username(&payload.username)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict("Username unavailable".to_string()));
    }

    let password_hash = hash_password(&payload.password)?;
    let mut user = UserAccount::new(
        payload.username.clone(),
        payload.email.clone().filter(|e| !e.is_empty()),
        password_hash,
    );
    user_repo.create_user(&user).await?;

    // Mark invite as used (any mode, if a token was validated above)
    if let Some(ref invite) = invite_opt {
        let invite_repo = InviteRepository::new(pool.clone());
        let now = chrono::Utc::now().to_rfc3339();
        let _ = invite_repo
            .mark_used(&invite.token, &user.user_id, &now)
            .await;
    }

    tracing::info!(
    target: "security",
    event_category = "auth",
    event_action = "user_registered",
    event_outcome = "success",
    user_id = %user.user_id,
    "New user registered"
    );

    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            user: UserResponse {
                user_id: user.user_id,
                email: user.email,
                username: user.username,
                display_name: None,
                avatar_url: None,
                verified: false,
                admin: false,
            },
        }),
    ))
}

/// Get public auth configuration, tells the frontend how registration is handled.
/// GET /api/auth/config
pub async fn get_auth_config(
    State((pool, settings)): State<AppState>,
) -> Result<Json<AuthConfigResponse>, AppError> {
    let config_repo = SiteConfigRepository::new(pool.clone());
    let config = config_repo.get().await.map_err(|e| {
        tracing::error!("Failed to fetch site config: {:?}", e);
        AppError::Internal("Failed to fetch site configuration".to_string())
    })?;
    let smtp_ok = EmailSettingsRepository::new(pool)
        .get()
        .await
        .map(|es| crate::utils::email::smtp_is_configured_db(&es))
        .unwrap_or(false);
    Ok(Json(AuthConfigResponse {
        registration_mode: config.registration_mode,
        password_reset_email_available: config.password_reset_email_enabled != 0 && smtp_ok,
        password_reset_signing_key_available: config.password_reset_signing_key_enabled != 0,
    }))
}

/// Submit a registration application (application mode only).
/// POST /api/auth/apply
pub async fn submit_application(
    State((pool, _settings)): State<AppState>,
    Json(payload): Json<SubmitApplicationRequest>,
) -> Result<(StatusCode, Json<ApplicationResponse>), AppError> {
    payload.validate().map_err(AppError::from)?;
    if payload.password.is_none() && payload.email.is_none() {
        return Err(AppError::BadRequest(
            "Either Password or Email are mandatory".to_string(),
        ));
    }

    let config_repo = SiteConfigRepository::new(pool.clone());
    let config = config_repo.get().await.map_err(|e| {
        tracing::error!("Failed to fetch site config: {:?}", e);
        AppError::Internal("Failed to fetch site configuration".to_string())
    })?;

    if config.registration_mode != "application" {
        return Err(AppError::BadRequest(
            "Registration applications are not enabled".to_string(),
        ));
    }

    let user_repo = UserRepository::new(pool.clone());

    // Username must be unique among existing accounts
    if user_repo
        .find_by_username(&payload.username)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict("Username already taken".to_string()));
    }

    // Optionally check email uniqueness if one was provided
    if let Some(ref email) = payload.email {
        if user_repo.find_by_email(email).await?.is_some() {
            return Err(AppError::Conflict("Email already registered".to_string()));
        }
    }

    let app_repo = RegisterApplicationRepository::new(pool.clone());

    // Prevent duplicate pending applications for the same username
    if app_repo
        .find_pending_by_username(&payload.username)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict(
            "A pending application for this username already exists".to_string(),
        ));
    }

    let password_hash = if let Some(password) = payload.password {
        hash_password(&password).ok()
    } else {
        None
    };

    let application = RegisterApplication::new(
        payload.username,
        payload.email,
        password_hash,
        payload.display_name,
        payload.message,
        config.application_timeout_hours as i64,
    );

    app_repo.create(&application).await?;

    tracing::info!(
        "Registration application submitted: {}",
        application.application_id
    );

    let is_expired = application.is_expired();
    Ok((
        StatusCode::CREATED,
        Json(ApplicationResponse {
            application_id: application.application_id,
            username: application.username,
            email: application.email,
            display_name: application.display_name,
            message: application.message,
            status: application.status,
            created_at: application.created_at,
            expires_at: application.expires_at,
            reviewed_at: application.reviewed_at,
            reviewed_by: application.reviewed_by,
            is_expired,
        }),
    ))
}

/// User login
/// POST /api/auth/login
pub async fn login(
    State((pool, settings)): State<AppState>,
    jar: CookieJar,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(payload): Json<LoginRequest>,
) -> Result<(CookieJar, Json<AuthResponse>), AppError> {
    // Validate request
    payload.validate().map_err(AppError::from)?;

    let user_repo = UserRepository::new(pool.clone());

    // Find user by email or username (whichever identifier was provided).
    let user = user_repo
        .find_by_identifier(&payload.identifier)
        .await?
        .ok_or(AppError::Unauthorized)?;

    // Verify password
    let valid = verify_password(&payload.password, &user.password_hash)?;
    if !valid {
        tracing::warn!(
        target: "security",
        event_category = "auth",
        event_action = "login_failed",
        event_outcome = "failure",
        identifier = %payload.identifier,
        "Failed login attempt - invalid password"
        );
        return Err(AppError::Unauthorized);
    }

    // Check if account is active
    if user.active == 0 {
        tracing::warn!(
        target: "security",
        event_category = "auth",
        event_action = "login_failed",
        event_outcome = "failure",
        user_id = %user.user_id,
        "Login attempt on deactivated account"
        );
        return Err(AppError::Forbidden);
    }

    // Collect device context for this session
    let ip_address = client_ip;

    let user_agent = headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let peer_ip = ip_address.as_ref().and_then(|s| s.parse().ok());
    let fingerprint = generate_fingerprint(&headers, peer_ip);

    // Fire a security event if this fingerprint has never been seen for this user.
    // Uses a persistent known_devices table so recognition survives logouts.
    let fingerprint_known = user_repo
        .fingerprint_known_for_user(&user.user_id, &fingerprint)
        .await
        .unwrap_or(true); // on error, assume known to avoid spam

    if !fingerprint_known {
        let ip_display = ip_address.as_deref().unwrap_or("unknown");
        let ua_display = user_agent.as_deref().unwrap_or("unknown client");
        let body = format!(
            "A new device or browser signed in to your account.\n\nIP: {}\nClient: {}\n\nIf this was not you, revoke this session immediately from Account Security \u{2192} Active Sessions.",
            ip_display, ua_display
        );
        let meta = serde_json::json!({
            "ip_address": ip_address,
            "user_agent": user_agent,
            "fingerprint": fingerprint,
        });
        let event_repo = EventRepository::new(pool.clone());
        let _ = event_repo
            .create(
                &user.user_id,
                "security",
                "new_device_login",
                "warning",
                "New sign-in from an unrecognized device",
                &body,
                Some(&meta.to_string()),
            )
            .await;
    }

    // Record the device as known (persists across logouts)
    let _ = user_repo
        .upsert_known_device(
            &user.user_id,
            &fingerprint,
            ip_address.as_deref(),
            user_agent.as_deref(),
        )
        .await;

    // Create session with device context
    let mut session = Session::new(user.user_id.clone(), settings.session_duration_days);
    session.ip_address = ip_address;
    session.user_agent = user_agent;
    session.fingerprint = Some(fingerprint);
    user_repo.create_session(&session).await?;

    // Create session cookie
    // Only set Secure flag when actually served over HTTPS.
    // A plain-HTTP deployment (e.g. behind a reverse proxy that handles TLS
    // at the edge) must not set Secure or the browser silently drops the cookie.
    let https = settings.base_url.starts_with("https://");
    let cookie = Cookie::build((SESSION_COOKIE_NAME, session.session_id.clone()))
        .path("/")
        .http_only(true)
        .secure(https)
        .same_site(SameSite::Strict)
        .max_age(Duration::days(settings.session_duration_days))
        .build();

    let jar = jar.add(cookie);

    tracing::info!(
    target: "security",
    event_category = "auth",
    event_action = "login_success",
    event_outcome = "success",
    user_id = %user.user_id,
    session_id = %session.session_id,
    "User logged in successfully"
    );

    let av_url = avatar_url(&user.avatar_media_id);
    Ok((
        jar,
        Json(AuthResponse {
            user: UserResponse {
                user_id: user.user_id,
                // Return email on login regardless of visibility, it's the user's own data.
                email: user.email,
                username: user.username,
                display_name: user.display_name,
                avatar_url: av_url,
                verified: user.verified != 0,
                admin: user.admin != 0,
            },
        }),
    ))
}

/// User logout
/// POST /api/auth/logout
pub async fn logout(
    State((pool, _settings)): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, impl IntoResponse), AppError> {
    // Get session ID from cookie
    if let Some(session_cookie) = jar.get(SESSION_COOKIE_NAME) {
        let session_id = session_cookie.value();
        let user_repo = UserRepository::new(pool);

        // Delete session from database
        user_repo.delete_session(session_id).await?;

        tracing::info!(
        target: "security",
        event_category = "auth",
        event_action = "logout",
        event_outcome = "success",
        session_id = %session_id,
        "User logged out"
        );
    }

    // Remove cookie by setting it with immediate expiration
    let cookie = Cookie::build((SESSION_COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .max_age(Duration::seconds(0))
        .build();

    let jar = jar.remove(cookie);

    Ok((jar, StatusCode::NO_CONTENT))
}

/// Request password reset via email token
/// POST /api/auth/request-password-reset
pub async fn request_password_reset(
    State((pool, settings)): State<AppState>,
    Json(payload): Json<RequestPasswordResetRequest>,
) -> Result<Json<MessageResponse>, AppError> {
    payload.validate().map_err(AppError::from)?;

    // Check that email reset is enabled
    let config_repo = SiteConfigRepository::new(pool.clone());
    let config = config_repo.get().await.map_err(|e| {
        tracing::error!("Failed to fetch site config: {:?}", e);
        AppError::Internal("Failed to fetch site configuration".to_string())
    })?;
    if config.password_reset_email_enabled == 0 {
        return Err(AppError::BadRequest(
            "Email password reset is not enabled on this server".to_string(),
        ));
    }

    let email_settings = EmailSettingsRepository::new(pool.clone())
        .get()
        .await
        .map_err(|_| AppError::Internal("Failed to fetch email settings".to_string()))?;

    if !crate::utils::email::smtp_is_configured_db(&email_settings) {
        return Err(AppError::BadRequest(
            "Email is not configured on this server".to_string(),
        ));
    }

    // Fetch branding for the email template
    let theme = ThemeRepository::new(pool.clone())
        .get_active()
        .await
        .ok()
        .flatten();
    let site_title = theme.as_ref().map_or("Magnolia", |t| &t.site_title);
    let accent = theme.as_ref().map_or("#2563eb", |t| &t.color_accent);

    let user_repo = UserRepository::new(pool);

    // Find user by email (don't reveal whether the account exists)
    if let Some(user) = user_repo.find_by_email(&payload.email).await? {
        user_repo
            .invalidate_user_password_resets(&user.user_id)
            .await?;

        let reset = PasswordReset::new(user.user_id.clone());
        user_repo.create_password_reset_token(&reset).await?;

        let reset_link = format!(
            "{}/#reset-password/{}",
            settings.base_url.trim_end_matches('/'),
            reset.token
        );
        let subject = format!("{} – Password reset request", site_title);

        let (text_body, html_body) = build_reset_email(site_title, accent, reset_link);

        if let Err(e) = crate::utils::email::send_email_db_html(
            &email_settings,
            &payload.email,
            &subject,
            &text_body,
            &html_body,
        )
        .await
        {
            tracing::error!(
                "Failed to send password reset email to {}: {e}",
                payload.email
            );
        }

        tracing::info!(
            target: "security",
            event_category = "auth",
            event_action = "password_reset_requested",
            event_outcome = "success",
            user_id = %user.user_id,
            "Password reset email sent"
        );
    }

    // Always return success to prevent email enumeration
    Ok(Json(MessageResponse {
        message: "If this email exists, a password reset link has been sent".to_string(),
    }))
}

/// Validate password reset token
/// POST /api/auth/validate-password-reset
pub async fn validate_password_reset(
    State((pool, _settings)): State<AppState>,
    Json(payload): Json<ValidatePasswordResetRequest>,
) -> Result<Json<MessageResponse>, AppError> {
    let user_repo = UserRepository::new(pool);

    // Find token
    let reset = user_repo
        .find_password_reset_token(&payload.token)
        .await?
        .ok_or_else(|| AppError::BadRequest("Invalid reset token".to_string()))?;

    // Check if used
    if reset.used != 0 {
        return Err(AppError::BadRequest(
            "Reset token has already been used".to_string(),
        ));
    }

    // Check expiration
    if reset.is_expired() {
        return Err(AppError::BadRequest("Reset token has expired".to_string()));
    }

    Ok(Json(MessageResponse {
        message: "Token is valid".to_string(),
    }))
}

/// Reset password with token
/// POST /api/auth/reset-password
pub async fn reset_password(
    State((pool, _settings)): State<AppState>,
    Json(payload): Json<ResetPasswordRequest>,
) -> Result<Json<MessageResponse>, AppError> {
    // Validate request
    payload.validate().map_err(AppError::from)?;

    // Validate password strength
    validate_password_strength(&payload.password)?;

    let user_repo = UserRepository::new(pool);

    // Find and validate token
    let reset = user_repo
        .find_password_reset_token(&payload.token)
        .await?
        .ok_or_else(|| AppError::BadRequest("Invalid reset token".to_string()))?;

    if reset.used != 0 {
        return Err(AppError::BadRequest(
            "Reset token has already been used".to_string(),
        ));
    }

    if reset.is_expired() {
        return Err(AppError::BadRequest("Reset token has expired".to_string()));
    }

    // Hash new password
    let password_hash = hash_password(&payload.password)?;

    // Update password
    user_repo
        .update_password(&reset.user_id, &password_hash)
        .await?;

    // Invalidate all reset tokens for this user
    user_repo
        .invalidate_user_password_resets(&reset.user_id)
        .await?;

    // Invalidate all existing sessions (force re-login)
    user_repo.delete_user_sessions(&reset.user_id).await?;

    tracing::info!(
    target: "security",
    event_category = "auth",
    event_action = "password_reset_completed",
    event_outcome = "success",
    user_id = %reset.user_id,
    "Password reset completed successfully"
    );

    Ok(Json(MessageResponse {
        message: "Password reset successfully. Please log in with your new password.".to_string(),
    }))
}

/// Get current user (for authenticated requests)
/// GET /api/auth/me
pub async fn get_current_user(
    axum::Extension(auth): axum::Extension<crate::middleware::auth::AuthMiddleware>,
) -> Result<Json<UserResponse>, AppError> {
    let user = auth.user;
    let av_url = avatar_url(&user.avatar_media_id);

    Ok(Json(UserResponse {
        user_id: user.user_id,
        email: user.email, // own data, always returned
        username: user.username,
        display_name: user.display_name,
        avatar_url: av_url,
        verified: user.verified != 0,
        admin: user.admin != 0,
    }))
}

/// List users
/// GET /api/users
pub async fn list_users(
    State((pool, _settings)): State<AppState>,
    axum::Extension(_auth): axum::Extension<crate::middleware::auth::AuthMiddleware>,
    Query(params): Query<UserListQuery>,
) -> Result<Json<UserListResponse>, AppError> {
    let repo = UserRepository::new(pool);

    let (users, total) = if let Some(ref q) = params.q {
        if q.is_empty() {
            repo.find_all_paginated(params.limit, params.offset).await?
        } else {
            repo.search_by_email(q, params.limit, params.offset).await?
        }
    } else {
        repo.find_all_paginated(params.limit, params.offset).await?
    };

    let items = users
        .into_iter()
        .map(|u| {
            let av_url = avatar_url(&u.avatar_media_id);
            UserListItem {
                user_id: u.user_id,
                email: if u.email_visible != 0 { u.email } else { None },
                username: u.username,
                display_name: u.display_name,
                avatar_url: av_url,
            }
        })
        .collect();

    Ok(Json(UserListResponse {
        users: items,
        total,
    }))
}

/// Get user profile
/// GET /api/users/:user_id/profile
pub async fn get_profile(
    State((pool, _settings)): State<AppState>,
    axum::Extension(_auth): axum::Extension<crate::middleware::auth::AuthMiddleware>,
    Path(user_id): Path<String>,
) -> Result<Json<ProfileResponse>, AppError> {
    let repo = UserRepository::new(pool);

    let user = repo
        .find_by_id(&user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    let is_own_profile = _auth.user.user_id == user.user_id;
    Ok(Json(ProfileResponse {
        user_id: user.user_id,
        email: if is_own_profile || user.email_visible != 0 {
            user.email
        } else {
            None
        },
        email_visible: user.email_visible != 0,
        display_name: user.display_name,
        username: user.username,
        bio: user.bio,
        avatar_url: avatar_url(&user.avatar_media_id),
        location: user.location,
        website: user.website,
        public_key: user.public_key,
        created_at: user.created_at,
    }))
}

/// Update own profile
/// PUT /api/profile
pub async fn update_profile(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<crate::middleware::auth::AuthMiddleware>,
    Json(payload): Json<UpdateProfileRequest>,
) -> Result<Json<ProfileResponse>, AppError> {
    payload.validate().map_err(AppError::from)?;

    let repo = UserRepository::new(pool.clone());

    // Validate avatar_media_id if provided
    if let Some(ref media_id) = payload.avatar_media_id {
        if !media_id.is_empty() {
            let media_repo = MediaRepository::new(pool.clone());
            let media = media_repo
                .get_by_id(media_id)
                .await
                .map_err(|_| AppError::BadRequest("Invalid avatar media".to_string()))?;
            match media {
                Some(m) if m.owner_id == auth.user.user_id => {}
                _ => return Err(AppError::BadRequest("Invalid avatar media".to_string())),
            }
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    let dn = payload.display_name.as_deref();
    let bio = payload.bio.as_deref();
    let av = payload.avatar_media_id.as_deref();
    let loc = payload.location.as_deref();
    let web = payload.website.as_deref();

    repo.update_profile(&auth.user.user_id, dn, bio, av, loc, web, &now)
        .await?;

    // Re-fetch updated user
    let user = repo
        .find_by_id(&auth.user.user_id)
        .await?
        .ok_or_else(|| AppError::Internal("User not found after update".to_string()))?;

    Ok(Json(ProfileResponse {
        user_id: user.user_id,
        email: user.email, // own profile update, always return own email
        email_visible: user.email_visible != 0,
        display_name: user.display_name,
        username: user.username,
        bio: user.bio,
        avatar_url: avatar_url(&user.avatar_media_id),
        location: user.location,
        website: user.website,
        public_key: user.public_key,
        created_at: user.created_at,
    }))
}

/// Change own password while logged in
/// POST /api/auth/change-password
pub async fn change_password(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<crate::middleware::auth::AuthMiddleware>,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Json<MessageResponse>, AppError> {
    payload.validate().map_err(AppError::from)?;

    // Validate new password strength
    validate_password_strength(&payload.new_password)?;

    let repo = UserRepository::new(pool);

    // Re-fetch the full user record (with password hash) to verify current password
    let user = repo
        .find_by_id(&auth.user.user_id)
        .await?
        .ok_or_else(|| AppError::Internal("User not found".to_string()))?;

    // Verify current password is correct before allowing the change
    if !verify_password(&payload.current_password, &user.password_hash)? {
        return Err(AppError::Unauthorized);
    }

    let new_hash = hash_password(&payload.new_password)?;
    repo.update_password(&auth.user.user_id, &new_hash).await?;

    tracing::info!("User {} changed their password", auth.user.user_id);

    Ok(Json(MessageResponse {
        message: "Password updated successfully".to_string(),
    }))
}

/// Retrieve the authenticated user's passphrase-encrypted E2E key blob.
/// GET /api/auth/me/e2e-key
pub async fn get_e2e_key_blob(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<crate::middleware::auth::AuthMiddleware>,
) -> Result<Json<E2EKeyBlobResponse>, AppError> {
    let repo = UserRepository::new(pool);
    let blob = repo.get_e2e_key_blob(&auth.user.user_id).await?;
    Ok(Json(E2EKeyBlobResponse { e2e_key_blob: blob }))
}

/// Persist the authenticated user's passphrase-encrypted E2E key blob.
/// The server stores it opaquely, it cannot decrypt it.
/// PUT /api/auth/me/e2e-key
pub async fn set_e2e_key_blob(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<crate::middleware::auth::AuthMiddleware>,
    Json(payload): Json<SetE2EKeyBlobRequest>,
) -> Result<Json<MessageResponse>, AppError> {
    payload.validate().map_err(AppError::from)?;
    let repo = UserRepository::new(pool);
    repo.set_e2e_key_blob(&auth.user.user_id, &payload.e2e_key_blob)
        .await?;
    Ok(Json(MessageResponse {
        message: "E2E key blob stored".to_string(),
    }))
}

//  Password-reset signing key

/// GET /api/auth/me/password-reset-key/status
/// Returns whether the calling user has a signing key stored.
pub async fn get_password_reset_key_status(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<crate::middleware::auth::AuthMiddleware>,
) -> Result<Json<PasswordResetKeyStatus>, AppError> {
    let repo = UserRepository::new(pool);
    let key = repo
        .get_password_reset_signing_key(&auth.user.user_id)
        .await?;
    Ok(Json(PasswordResetKeyStatus {
        has_key: key.is_some(),
    }))
}

/// POST /api/auth/me/password-reset-key/generate
/// Generates (or re-generates) the user's HMAC signing key and returns it as a
/// downloadable payload. The raw key is never stored anywhere else; the server
/// retains it to verify future reset requests.
pub async fn generate_password_reset_key(
    State((pool, settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<crate::middleware::auth::AuthMiddleware>,
) -> Result<Json<PasswordResetKeyResponse>, AppError> {
    // Check that signing-key reset is enabled
    let config_repo = SiteConfigRepository::new(pool.clone());
    let config = config_repo.get().await.map_err(|e| {
        tracing::error!("Failed to fetch site config: {:?}", e);
        AppError::Internal("Failed to fetch site configuration".to_string())
    })?;
    if config.password_reset_signing_key_enabled == 0 {
        return Err(AppError::BadRequest(
            "Signing-key password reset is not enabled on this server".to_string(),
        ));
    }

    // Generate 32 random bytes
    let mut key_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut key_bytes);
    let key_b64 = B64.encode(key_bytes);

    let repo = UserRepository::new(pool);
    repo.set_password_reset_signing_key(&auth.user.user_id, &key_b64)
        .await?;

    let generated_at = chrono::Utc::now().to_rfc3339();
    tracing::info!(
        target: "security",
        event_category = "auth",
        event_action = "reset_signing_key_generated",
        user_id = %auth.user.user_id,
        "Password reset signing key generated"
    );

    Ok(Json(PasswordResetKeyResponse {
        user_id: auth.user.user_id.clone(),
        username: auth.user.username.clone(),
        key: key_b64,
        generated_at,
    }))
}

/// DELETE /api/auth/me/password-reset-key
/// Revokes the user's signing key so it can no longer be used for password reset.
pub async fn revoke_password_reset_key(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<crate::middleware::auth::AuthMiddleware>,
) -> Result<Json<MessageResponse>, AppError> {
    let repo = UserRepository::new(pool);
    repo.clear_password_reset_signing_key(&auth.user.user_id)
        .await?;
    tracing::info!(
        target: "security",
        event_category = "auth",
        event_action = "reset_signing_key_revoked",
        user_id = %auth.user.user_id,
        "Password reset signing key revoked"
    );
    Ok(Json(MessageResponse {
        message: "Recovery key revoked".to_string(),
    }))
}

/// POST /api/auth/reset-password-with-key
/// Public endpoint. Resets the password if the HMAC signature over
/// "magnolia-reset|{timestamp}|{new_password}" verifies against the stored key.
pub async fn reset_password_with_key(
    State((pool, _settings)): State<AppState>,
    Json(payload): Json<ResetPasswordWithKeyRequest>,
) -> Result<Json<MessageResponse>, AppError> {
    payload.validate().map_err(AppError::from)?;

    // Check that signing-key reset is enabled
    let config_repo = SiteConfigRepository::new(pool.clone());
    let config = config_repo.get().await.map_err(|e| {
        tracing::error!("Failed to fetch site config: {:?}", e);
        AppError::Internal("Failed to fetch site configuration".to_string())
    })?;
    if config.password_reset_signing_key_enabled == 0 {
        return Err(AppError::BadRequest(
            "Signing-key password reset is not enabled on this server".to_string(),
        ));
    }

    // Reject stale or future timestamps (±5 minutes)
    let now_secs = chrono::Utc::now().timestamp();
    if (now_secs - payload.timestamp).abs() > 300 {
        return Err(AppError::BadRequest(
            "Request timestamp is out of range".to_string(),
        ));
    }

    validate_password_strength(&payload.new_password)?;

    let user_repo = UserRepository::new(pool);

    // Fetch the stored signing key for this user
    let (user_id, stored_key_opt) = user_repo
        .find_by_id_with_signing_key(&payload.user_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("Invalid user".to_string()))?;

    let stored_key_b64 = stored_key_opt.ok_or_else(|| {
        AppError::BadRequest("No recovery key on file for this account".to_string())
    })?;

    // Verify HMAC-SHA256( key, "magnolia-reset|{timestamp}|{new_password}" )
    let key_bytes = B64
        .decode(&stored_key_b64)
        .map_err(|_| AppError::Internal("Stored key is malformed".to_string()))?;
    let msg = format!(
        "magnolia-reset|{}|{}",
        payload.timestamp, payload.new_password
    );
    let mut mac = Hmac::<Sha256>::new_from_slice(&key_bytes)
        .map_err(|_| AppError::Internal("HMAC key error".to_string()))?;
    mac.update(msg.as_bytes());
    let sig_bytes = B64
        .decode(&payload.signature)
        .map_err(|_| AppError::BadRequest("Invalid signature encoding".to_string()))?;
    mac.verify_slice(&sig_bytes)
        .map_err(|_| AppError::BadRequest("Invalid recovery key or signature".to_string()))?;

    // All checks passed - update the password
    let password_hash = hash_password(&payload.new_password)?;
    user_repo.update_password(&user_id, &password_hash).await?;
    // Invalidate all sessions (force re-login)
    user_repo.delete_user_sessions(&user_id).await?;
    // Optionally revoke the key so it can only be used once - user must regenerate
    user_repo.clear_password_reset_signing_key(&user_id).await?;

    tracing::info!(
        target: "security",
        event_category = "auth",
        event_action = "password_reset_with_key_completed",
        event_outcome = "success",
        user_id = %user_id,
        "Password reset via signing key completed"
    );

    Ok(Json(MessageResponse {
        message: "Password reset successfully. Please log in with your new password.".to_string(),
    }))
}

/// Store the authenticated user's E2E public key
/// PUT /api/auth/me/public-key
pub async fn update_public_key(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<crate::middleware::auth::AuthMiddleware>,
    Json(payload): Json<UpdatePublicKeyRequest>,
) -> Result<Json<MessageResponse>, AppError> {
    payload.validate().map_err(AppError::from)?;

    let repo = UserRepository::new(pool);
    repo.update_public_key(&auth.user.user_id, &payload.public_key)
        .await?;

    Ok(Json(MessageResponse {
        message: "Public key updated".to_string(),
    }))
}

/// GET /api/auth/sessions
/// List all sessions for the authenticated user.
pub async fn list_sessions(
    State((pool, _settings)): State<AppState>,
    jar: CookieJar,
    Extension(auth): Extension<crate::middleware::auth::AuthMiddleware>,
) -> Result<Json<Vec<SessionResponse>>, AppError> {
    let current_session_id = jar.get(SESSION_COOKIE_NAME).map(|c| c.value().to_string());

    let repo = UserRepository::new(pool);
    let sessions = repo.list_sessions_for_user(&auth.user.user_id).await?;

    let responses = sessions
        .into_iter()
        .map(|s| {
            let is_current = current_session_id
                .as_deref()
                .map(|id| id == s.session_id)
                .unwrap_or(false);
            SessionResponse {
                session_id: s.session_id,
                created_at: s.created_at,
                expires_at: s.expires_at,
                ip_address: s.ip_address,
                user_agent: s.user_agent,
                is_current,
            }
        })
        .collect();

    Ok(Json(responses))
}

/// DELETE /api/auth/sessions/:id
/// Revoke a specific session. Users can only revoke their own sessions.
/// Also evicts the session's fingerprint from known_devices so that if the
/// same device logs in again it will trigger a new-device event.
pub async fn revoke_session(
    State((pool, _settings)): State<AppState>,
    Extension(auth): Extension<crate::middleware::auth::AuthMiddleware>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, AppError> {
    let repo = UserRepository::new(pool);

    // Fetch session first to get its fingerprint before deleting
    let session = repo
        .find_session(&session_id)
        .await?
        .filter(|s| s.user_id == auth.user.user_id)
        .ok_or_else(|| AppError::NotFound("Session not found".to_string()))?;

    repo.delete_session_for_user(&session_id, &auth.user.user_id)
        .await?;

    // Evict fingerprint so next login from this device triggers a new-device event
    if let Some(fp) = session.fingerprint {
        let _ = repo.remove_known_device(&auth.user.user_id, &fp).await;
    }

    Ok(StatusCode::NO_CONTENT)
}
