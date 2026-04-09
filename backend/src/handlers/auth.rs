//! Authentication handlers for user registration, login, and account management

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use sqlx::AnyPool;
use std::sync::Arc;
use time::Duration;
use validator::Validate;

use crate::config::Settings;
use crate::utils::crypto::{hash_password, verify_password};
use crate::utils::password::validate_password_strength;
use magnolia_common::errors::AppError;
use magnolia_common::models::{
    EmailVerification, PasswordReset, RegisterApplication, Session, UserAccount,
};
use magnolia_common::repositories::{
    InviteRepository, MediaRepository, RegisterApplicationRepository, SiteConfigRepository,
    UserRepository,
};
use magnolia_common::schemas::{
    ApplicationResponse, AuthConfigResponse, AuthResponse, ChangePasswordRequest,
    E2EKeyBlobResponse, LoginRequest, MessageResponse, ProfileResponse, RegisterRequest,
    RequestPasswordResetRequest, ResendVerificationRequest, ResetPasswordRequest,
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

    // Validate the invite token (if provided) — a valid token bypasses mode restrictions
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

    // Apply registration mode restrictions — a valid invite grants access in any mode
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

    // Mark invite as used (any mode — if a token was validated above)
    if let Some(ref invite) = invite_opt {
        let invite_repo = InviteRepository::new(pool.clone());
        let now = chrono::Utc::now().to_rfc3339();
        let _ = invite_repo
            .mark_used(&invite.token, &user.user_id, &now)
            .await;
    }

    // Only create email verification token if user provided an email.
    if user.email.is_some() {
        let verification = EmailVerification::new(user.user_id.clone());
        user_repo.create_verification_token(&verification).await?;
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

/// Get public auth configuration — tells the frontend how registration is handled.
/// GET /api/auth/config
pub async fn get_auth_config(
    State((pool, _settings)): State<AppState>,
) -> Result<Json<AuthConfigResponse>, AppError> {
    let repo = SiteConfigRepository::new(pool);
    let config = repo.get().await.map_err(|e| {
        tracing::error!("Failed to fetch site config: {:?}", e);
        AppError::Internal("Failed to fetch site configuration".to_string())
    })?;
    Ok(Json(AuthConfigResponse {
        registration_mode: config.registration_mode,
    }))
}

/// Submit a registration application (application mode only).
/// POST /api/auth/apply
pub async fn submit_application(
    State((pool, _settings)): State<AppState>,
    Json(payload): Json<SubmitApplicationRequest>,
) -> Result<(StatusCode, Json<ApplicationResponse>), AppError> {
    payload.validate().map_err(AppError::from)?;

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
    if user_repo.find_by_email(&payload.email).await?.is_some() {
        return Err(AppError::Conflict("Email already registered".to_string()));
    }

    let app_repo = RegisterApplicationRepository::new(pool.clone());
    if app_repo
        .find_pending_by_email(&payload.email)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict(
            "A pending application for this email already exists".to_string(),
        ));
    }

    let application = RegisterApplication::new(
        payload.email,
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

/// Verify email address with token
/// POST /api/auth/verify-email
pub async fn verify_email(
    State((pool, _settings)): State<AppState>,
    Json(payload): Json<VerifyEmailRequest>,
) -> Result<Json<MessageResponse>, AppError> {
    let user_repo = UserRepository::new(pool);

    // Find token
    let verification = user_repo
        .find_verification_token(&payload.token)
        .await?
        .ok_or_else(|| AppError::BadRequest("Invalid verification token".to_string()))?;

    // Check if already used
    if verification.used != 0 {
        return Err(AppError::BadRequest(
            "Token has already been used".to_string(),
        ));
    }

    // Check expiration
    if verification.is_expired() {
        return Err(AppError::BadRequest(
            "Verification token has expired".to_string(),
        ));
    }

    // Mark user as verified
    user_repo.mark_verified(&verification.user_id).await?;

    // Mark token as used
    user_repo.mark_token_used(&payload.token).await?;

    tracing::info!(
    target: "security",
    event_category = "auth",
    event_action = "email_verified",
    event_outcome = "success",
    user_id = %verification.user_id,
    "Email verified successfully"
    );

    Ok(Json(MessageResponse {
        message: "Email verified successfully".to_string(),
    }))
}

/// Resend email verification
/// POST /api/auth/resend-verification
pub async fn resend_verification(
    State((pool, _settings)): State<AppState>,
    Json(payload): Json<ResendVerificationRequest>,
) -> Result<Json<MessageResponse>, AppError> {
    // Validate request
    payload.validate().map_err(AppError::from)?;

    let user_repo = UserRepository::new(pool);

    // Find user by email
    let user = user_repo
        .find_by_email(&payload.email)
        .await?
        .ok_or_else(|| {
            // Generic message to prevent email enumeration
            AppError::BadRequest(
                "If this email exists, a verification email has been sent".to_string(),
            )
        })?;

    // Check if already verified
    if user.verified != 0 {
        return Ok(Json(MessageResponse {
            message: "If this email exists, a verification email has been sent".to_string(),
        }));
    }

    // Create new verification token
    let verification = EmailVerification::new(user.user_id.clone());
    user_repo.create_verification_token(&verification).await?;

    // TODO: Send verification email
    tracing::info!(
    target: "auth",
    user_id = %user.user_id,
    "Resent email verification token"
    );

    Ok(Json(MessageResponse {
        message: "If this email exists, a verification email has been sent".to_string(),
    }))
}

/// User login
/// POST /api/auth/login
pub async fn login(
    State((pool, settings)): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<LoginRequest>,
) -> Result<(CookieJar, Json<AuthResponse>), AppError> {
    // Validate request
    payload.validate().map_err(AppError::from)?;

    let user_repo = UserRepository::new(pool);

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

    // Create session
    let session = Session::new(user.user_id.clone(), settings.session_duration_days);
    user_repo.create_session(&session).await?;

    // Create session cookie
    let cookie = Cookie::build((SESSION_COOKIE_NAME, session.session_id.clone()))
        .path("/")
        .http_only(true)
        .secure(settings.env != "development")
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
                // Return email on login regardless of visibility — it's the user's own data.
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

/// Request password reset
/// POST /api/auth/request-password-reset
pub async fn request_password_reset(
    State((pool, _settings)): State<AppState>,
    Json(payload): Json<RequestPasswordResetRequest>,
) -> Result<Json<MessageResponse>, AppError> {
    // Validate request
    payload.validate().map_err(AppError::from)?;

    let user_repo = UserRepository::new(pool);

    // Find user by email (but don't reveal if found)
    if let Some(user) = user_repo.find_by_email(&payload.email).await? {
        // Invalidate any existing reset tokens
        user_repo
            .invalidate_user_password_resets(&user.user_id)
            .await?;

        // Create new reset token
        let reset = PasswordReset::new(user.user_id.clone());
        user_repo.create_password_reset_token(&reset).await?;

        // TODO: Send password reset email
        tracing::info!(
        target: "auth",
        user_id = %user.user_id,
        "Password reset token created (send via email in production)"
        );

        tracing::info!(
        target: "security",
        event_category = "auth",
        event_action = "password_reset_requested",
        event_outcome = "success",
        user_id = %user.user_id,
        "Password reset requested"
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
        email: user.email, // own profile update — always return own email
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
/// The server stores it opaquely — it cannot decrypt it.
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
