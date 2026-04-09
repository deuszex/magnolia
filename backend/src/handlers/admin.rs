//! Admin handlers — site configuration, user management, and invite management

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use sqlx::AnyPool;
use std::sync::Arc;
use validator::Validate;

use crate::config::Settings;
use crate::middleware::auth::AuthMiddleware;
use crate::utils::crypto::hash_password;
use crate::utils::email::{
    send_email, send_email_db, send_email_db_html, send_email_html, smtp_is_configured,
    smtp_is_configured_db,
};
use crate::utils::password::validate_password_strength;
use magnolia_common::errors::AppError;
use magnolia_common::models::{Invite, PasswordReset, RegisterApplication, UserAccount};
use magnolia_common::repositories::{
    EmailSettingsRepository, InviteRepository, RegisterApplicationRepository, SiteConfigRepository,
    ThemeRepository, UserRepository,
};
use magnolia_common::models::StunServer;
use magnolia_common::repositories::StunServerRepository;
use magnolia_common::schemas::{
    AdminCreateUserRequest, AdminUpdateUserRequest, AdminUserListItem, AdminUserListQuery,
    AdminUserListResponse, ApplicationListQuery, ApplicationListResponse, ApplicationResponse,
    ApproveApplicationResponse, CreateStunServerRequest, CreateInviteRequest,
    EmailSettingsResponse, InviteListQuery, InviteListResponse, InviteResponse, MessageResponse,
    SendEmailInvitesRequest, SendEmailInvitesResponse, SiteConfigResponse, StunServerResponse,
    UpdateEmailSettingsRequest, UpdateSiteConfigRequest, UpdateStunServerRequest,
};

type AppState = (AnyPool, Arc<Settings>);

fn avatar_url(media_id: &Option<String>) -> Option<String> {
    media_id
        .as_ref()
        .map(|id| format!("/api/media/{}/thumbnail", id))
}

/// Minimal HTML escaping for embedding user content in email templates.
fn he(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Build invite email subject, plain-text body, and HTML body.
fn build_invite_email(
    site_name: &str,
    accent: &str,
    invite_link: &str,
    expires_hours: i64,
    personal_message: Option<&str>,
) -> (String, String, String) {
    let subject = format!("You've been invited to join {}", site_name);

    // Plain text
    let msg_text = personal_message
        .map(|m| format!("\nMessage from the team:\n{}\n", m))
        .unwrap_or_default();

    let text_body = format!(
        "You have been invited to join {site_name}.{msg_text}\n\
 Click the link below to register:\n{invite_link}\n\n\
 This invite expires in {expires_hours} hours.\n\
 If you did not expect this email you can safely ignore it.",
        site_name = site_name,
        msg_text = msg_text,
        invite_link = invite_link,
        expires_hours = expires_hours,
    );

    // HTML
    let msg_html = personal_message
 .map(|m| {
 let lines: String = m
 .lines()
 .map(|l| format!("{}<br>", he(l)))
 .collect::<Vec<_>>()
 .join("\n");
 format!(
 r#"<tr><td style="padding:0 32px 24px">
 <div style="background:#1e2330;border-left:3px solid {accent};border-radius:4px;padding:14px 16px">
 <p style="margin:0 0 6px;font-size:11px;font-weight:700;text-transform:uppercase;letter-spacing:0.06em;color:{accent}">Message from the team</p>
 <p style="margin:0;color:#cbd5e1;font-size:14px;line-height:1.6">{lines}</p>
 </div>
</td></tr>"#,
 accent = accent,
 lines = lines,
 )
 })
 .unwrap_or_default();

    let site_name_h = he(site_name);
    let link_h = he(invite_link);

    let html_body = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
 <meta charset="UTF-8">
 <meta name="viewport" content="width=device-width,initial-scale=1.0">
 <title>You're invited to {site_name_h}</title>
</head>
<body style="margin:0;padding:0;background:#0d0f14;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif">
 <table role="presentation" width="100%" cellpadding="0" cellspacing="0"
 style="background:#0d0f14;padding:48px 16px">
 <tr><td align="center">

 <!-- Card -->
 <table role="presentation" cellpadding="0" cellspacing="0"
 style="background:#181b24;border-radius:12px;max-width:560px;width:100%;overflow:hidden">

 <!-- Header -->
 <tr>
 <td style="background:{accent};padding:22px 32px">
 <span style="color:#ffffff;font-size:18px;font-weight:700;letter-spacing:-0.02em">{site_name_h}</span>
 </td>
 </tr>

 <!-- Headline -->
 <tr>
 <td style="padding:36px 32px 8px">
 <h1 style="margin:0 0 12px;font-size:26px;font-weight:700;color:#f1f5f9;line-height:1.2">
 You&#8217;ve been invited!
 </h1>
 <p style="margin:0;color:#94a3b8;font-size:15px;line-height:1.6">
 You have been invited to join
 <strong style="color:#e2e8f0">{site_name_h}</strong>.
 Click the button below to create your account.
 </p>
 </td>
 </tr>

 <!-- Optional personal message -->
 {msg_html}

 <!-- CTA button -->
 <tr>
 <td style="padding:24px 32px 8px">
 <table role="presentation" cellpadding="0" cellspacing="0">
 <tr>
 <td style="background:{accent};border-radius:8px">
 <a href="{link_h}"
 style="display:inline-block;padding:13px 30px;color:#ffffff;
 text-decoration:none;font-weight:600;font-size:15px">
 Accept Invitation
 </a>
 </td>
 </tr>
 </table>
 </td>
 </tr>

 <!-- Fallback link -->
 <tr>
 <td style="padding:16px 32px 0">
 <p style="margin:0 0 4px;color:#64748b;font-size:12px">Or copy this link into your browser:</p>
 <p style="margin:0;word-break:break-all;font-size:12px;font-family:monospace;color:{accent}">
 <a href="{link_h}" style="color:{accent};text-decoration:none">{link_h}</a>
 </p>
 </td>
 </tr>

 <!-- Expiry notice -->
 <tr>
 <td style="padding:20px 32px 28px">
 <p style="margin:0;color:#475569;font-size:12px;
 padding-top:16px;border-top:1px solid #252935;line-height:1.5">
 This invitation expires in <strong>{expires_hours} hours</strong>.
 If you did not expect this email you can safely ignore it.
 </p>
 </td>
 </tr>

 <!-- Footer -->
 <tr>
 <td style="background:#11131a;padding:14px 32px;border-top:1px solid #252935">
 <p style="margin:0;color:#334155;font-size:11px">
 {site_name_h} &mdash; Sent via secure invitation
 </p>
 </td>
 </tr>

 </table>
 </td></tr>
 </table>
</body>
</html>"#,
        site_name_h = site_name_h,
        accent = accent,
        link_h = link_h,
        msg_html = msg_html,
        expires_hours = expires_hours,
    );

    (subject, text_body, html_body)
}

// Site configuration

/// GET /api/admin/site-config
pub async fn get_site_config(
    State((pool, settings)): State<AppState>,
    Extension(_auth): Extension<AuthMiddleware>,
) -> Result<Json<SiteConfigResponse>, AppError> {
    let repo = SiteConfigRepository::new(pool.clone());
    let config = repo.get().await.map_err(|e| {
        tracing::error!("Failed to fetch site config: {:?}", e);
        AppError::Internal("Failed to fetch site configuration".to_string())
    })?;

    let email_repo = EmailSettingsRepository::new(pool);
    let smtp_db = email_repo
        .get()
        .await
        .map(|e| smtp_is_configured_db(&e))
        .unwrap_or(false);

    Ok(Json(build_site_config_response(config, &settings, smtp_db)))
}

/// PUT /api/admin/site-config
pub async fn update_site_config(
    State((pool, settings)): State<AppState>,
    Extension(_auth): Extension<AuthMiddleware>,
    Json(payload): Json<UpdateSiteConfigRequest>,
) -> Result<Json<SiteConfigResponse>, AppError> {
    payload.validate().map_err(AppError::from)?;

    if payload.encryption_at_rest_enabled == Some(true) {
        if settings.encryption_at_rest_key.is_none() {
            return Err(AppError::BadRequest(
                "Cannot enable encryption at rest: ENCRYPTION_AT_REST_KEY environment \
 variable is not set. Set a 64-character hex key (32 bytes) and restart \
 the server before enabling."
                    .to_string(),
            ));
        }
    }

    if let Some(ref path) = payload.media_storage_path {
        if !path.is_empty() {
            let path_exists = tokio::fs::metadata(path).await.is_ok();
            if !path_exists {
                tokio::fs::create_dir_all(path).await.map_err(|e| {
                    tracing::error!("Failed to create storage directory '{}': {:?}", path, e);
                    AppError::BadRequest(format!(
                        "Storage path '{}' does not exist and could not be created: {}",
                        path, e
                    ))
                })?;
            }
        }
    }

    let repo = SiteConfigRepository::new(pool.clone());
    let current = repo.get().await.map_err(|e| {
        tracing::error!("Failed to fetch site config: {:?}", e);
        AppError::Internal("Failed to fetch site configuration".to_string())
    })?;

    let text = payload
        .allow_text_posts
        .map(|b| b as i32)
        .unwrap_or(current.allow_text_posts);
    let image = payload
        .allow_image_posts
        .map(|b| b as i32)
        .unwrap_or(current.allow_image_posts);
    let video = payload
        .allow_video_posts
        .map(|b| b as i32)
        .unwrap_or(current.allow_video_posts);
    let file = payload
        .allow_file_posts
        .map(|b| b as i32)
        .unwrap_or(current.allow_file_posts);

    if text == 0 && image == 0 && video == 0 && file == 0 {
        return Err(AppError::BadRequest(
            "At least one content type must be enabled".to_string(),
        ));
    }

    if let Some(ref mode) = payload.registration_mode {
        if !matches!(mode.as_str(), "open" | "invite_only" | "application") {
            return Err(AppError::BadRequest(
                "registration_mode must be 'open', 'invite_only', or 'application'".to_string(),
            ));
        }
    }

    let updated = repo
        .update(
            payload.media_storage_path.as_deref(),
            payload.allow_text_posts.map(|b| b as i32),
            payload.allow_image_posts.map(|b| b as i32),
            payload.allow_video_posts.map(|b| b as i32),
            payload.allow_file_posts.map(|b| b as i32),
            payload.encryption_at_rest_enabled.map(|b| b as i32),
            payload.message_auto_delete_enabled.map(|b| b as i32),
            payload.message_auto_delete_delay_hours,
            payload.registration_mode.as_deref(),
            payload.application_timeout_hours,
            payload.enforce_invite_email.map(|b| b as i32),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to update site config: {:?}", e);
            AppError::Internal("Failed to update site configuration".to_string())
        })?;

    let smtp_db = EmailSettingsRepository::new(pool)
        .get()
        .await
        .map(|e| smtp_is_configured_db(&e))
        .unwrap_or(false);

    Ok(Json(build_site_config_response(
        updated, &settings, smtp_db,
    )))
}

fn build_site_config_response(
    config: magnolia_common::models::SiteConfig,
    settings: &Settings,
    smtp_db_configured: bool,
) -> SiteConfigResponse {
    SiteConfigResponse {
        media_storage_path: config.media_storage_path,
        allow_text_posts: config.allow_text_posts == 1,
        allow_image_posts: config.allow_image_posts == 1,
        allow_video_posts: config.allow_video_posts == 1,
        allow_file_posts: config.allow_file_posts == 1,
        encryption_at_rest_enabled: config.encryption_at_rest_enabled == 1,
        encryption_key_configured: settings.encryption_at_rest_key.is_some(),
        message_auto_delete_enabled: config.message_auto_delete_enabled == 1,
        message_auto_delete_delay_hours: config.message_auto_delete_delay_hours,
        registration_mode: config.registration_mode,
        application_timeout_hours: config.application_timeout_hours,
        enforce_invite_email: config.enforce_invite_email == 1,
        smtp_configured: smtp_db_configured || smtp_is_configured(settings),
        updated_at: config.updated_at,
    }
}

// Email settings

/// GET /api/admin/email-settings
pub async fn get_email_settings(
    State((pool, _settings)): State<AppState>,
    Extension(_auth): Extension<AuthMiddleware>,
) -> Result<Json<EmailSettingsResponse>, AppError> {
    let repo = EmailSettingsRepository::new(pool);
    let s = repo.get().await.map_err(|e| {
        tracing::error!("Failed to fetch email settings: {:?}", e);
        AppError::Internal("Failed to fetch email settings".to_string())
    })?;

    Ok(Json(EmailSettingsResponse {
        smtp_host: s.smtp_host,
        smtp_port: s.smtp_port,
        smtp_username: s.smtp_username,
        smtp_password_set: !s.smtp_password.is_empty(),
        smtp_from: s.smtp_from,
        smtp_secure: s.smtp_secure,
        updated_at: s.updated_at,
    }))
}

/// PUT /api/admin/email-settings
pub async fn update_email_settings(
    State((pool, _settings)): State<AppState>,
    Extension(_auth): Extension<AuthMiddleware>,
    Json(payload): Json<UpdateEmailSettingsRequest>,
) -> Result<Json<EmailSettingsResponse>, AppError> {
    payload.validate().map_err(AppError::from)?;

    if let Some(ref mode) = payload.smtp_secure {
        if !matches!(mode.as_str(), "tls" | "ssl" | "none") {
            return Err(AppError::BadRequest(
                "smtp_secure must be 'tls', 'ssl', or 'none'".to_string(),
            ));
        }
    }

    let repo = EmailSettingsRepository::new(pool);
    let mut current = repo.get().await.map_err(|e| {
        tracing::error!("Failed to fetch email settings: {:?}", e);
        AppError::Internal("Failed to fetch email settings".to_string())
    })?;

    if let Some(v) = payload.smtp_host {
        current.smtp_host = v;
    }
    if let Some(v) = payload.smtp_port {
        current.smtp_port = v;
    }
    if let Some(v) = payload.smtp_username {
        current.smtp_username = v;
    }
    if let Some(v) = payload.smtp_from {
        current.smtp_from = v;
    }
    if let Some(v) = payload.smtp_secure {
        current.smtp_secure = v;
    }
    // Only update password if a non-empty value was provided
    if let Some(ref v) = payload.smtp_password {
        if !v.is_empty() {
            current.smtp_password = v.clone();
        }
    }

    repo.update(&current).await.map_err(|e| {
        tracing::error!("Failed to update email settings: {:?}", e);
        AppError::Internal("Failed to update email settings".to_string())
    })?;

    Ok(Json(EmailSettingsResponse {
        smtp_host: current.smtp_host,
        smtp_port: current.smtp_port,
        smtp_username: current.smtp_username,
        smtp_password_set: !current.smtp_password.is_empty(),
        smtp_from: current.smtp_from,
        smtp_secure: current.smtp_secure,
        updated_at: current.updated_at,
    }))
}

// User management

/// GET /api/admin/users
/// List all users with full admin-visible fields.
pub async fn admin_list_users(
    State((pool, _settings)): State<AppState>,
    Extension(_auth): Extension<AuthMiddleware>,
    Query(query): Query<AdminUserListQuery>,
) -> Result<Json<AdminUserListResponse>, AppError> {
    let user_repo = UserRepository::new(pool);
    let limit = query.limit.min(200);

    let (users, total) = if let Some(ref q) = query.q {
        user_repo.search_by_email(q, limit, query.offset).await?
    } else {
        user_repo.find_all_paginated(limit, query.offset).await?
    };

    let items = users
        .into_iter()
        .map(|u| AdminUserListItem {
            avatar_url: avatar_url(&u.avatar_media_id),
            verified: u.verified == 1,
            admin: u.admin == 1,
            active: u.active == 1,
            user_id: u.user_id,
            email: u.email,
            display_name: u.display_name,
            username: u.username,
            created_at: u.created_at,
        })
        .collect();

    Ok(Json(AdminUserListResponse {
        users: items,
        total,
    }))
}

/// POST /api/admin/users
/// Create a user directly, bypassing the email verification flow.
pub async fn admin_create_user(
    State((pool, _settings)): State<AppState>,
    Extension(_auth): Extension<AuthMiddleware>,
    Json(payload): Json<AdminCreateUserRequest>,
) -> Result<(StatusCode, Json<AdminUserListItem>), AppError> {
    payload.validate().map_err(AppError::from)?;
    validate_password_strength(&payload.password)?;

    let user_repo = UserRepository::new(pool);

    if user_repo
        .find_by_username(&payload.username)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict("Username unavailable".to_string()));
    }
    if let Some(ref email) = payload.email {
        if user_repo.find_by_email(email).await?.is_some() {
            return Err(AppError::Conflict("Email already registered".to_string()));
        }
    }

    let password_hash = hash_password(&payload.password)?;

    let mut user = UserAccount::new(
        payload.username.clone(),
        payload.email.clone(),
        password_hash,
    );
    user.verified = if payload.verified { 1 } else { 0 };
    user.admin = if payload.admin { 1 } else { 0 };

    user_repo.create_user(&user).await?;

    tracing::info!("Admin created user: {}", user.user_id);

    let item = AdminUserListItem {
        user_id: user.user_id,
        email: user.email,
        display_name: user.display_name,
        username: user.username,
        avatar_url: None,
        verified: user.verified == 1,
        admin: user.admin == 1,
        active: user.active == 1,
        created_at: user.created_at,
    };

    Ok((StatusCode::CREATED, Json(item)))
}

/// DELETE /api/admin/users/:user_id
/// Delete a user account and all associated data.
pub async fn admin_delete_user(
    State((pool, _settings)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Path(user_id): Path<String>,
) -> Result<Json<MessageResponse>, AppError> {
    if user_id == auth.user.user_id {
        return Err(AppError::BadRequest(
            "Cannot delete your own account via admin API".to_string(),
        ));
    }

    let user_repo = UserRepository::new(pool);

    if user_repo.find_by_id(&user_id).await?.is_none() {
        return Err(AppError::NotFound("User not found".to_string()));
    }

    user_repo.delete_user(&user_id).await?;

    tracing::info!("Admin deleted user: {}", user_id);

    Ok(Json(MessageResponse {
        message: "User deleted".to_string(),
    }))
}

/// PATCH /api/admin/users/:user_id
/// Update a user's active or admin flags.
pub async fn admin_update_user(
    State((pool, _settings)): State<AppState>,
    Extension(_auth): Extension<AuthMiddleware>,
    Path(user_id): Path<String>,
    Json(payload): Json<AdminUpdateUserRequest>,
) -> Result<Json<AdminUserListItem>, AppError> {
    let user_repo = UserRepository::new(pool);

    let user = user_repo
        .find_by_id(&user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    if let Some(active) = payload.active {
        user_repo.set_active_status(&user_id, active).await?;
    }
    if let Some(admin) = payload.admin {
        user_repo.set_admin_status(&user_id, admin).await?;
    }

    // Re-fetch to return the updated state
    let updated = user_repo.find_by_id(&user_id).await?.unwrap_or(user);

    Ok(Json(AdminUserListItem {
        avatar_url: avatar_url(&updated.avatar_media_id),
        verified: updated.verified == 1,
        admin: updated.admin == 1,
        active: updated.active == 1,
        user_id: updated.user_id,
        email: updated.email,
        display_name: updated.display_name,
        username: updated.username,
        created_at: updated.created_at,
    }))
}

// Invite management

/// POST /api/admin/invites
/// Create an invite link.
pub async fn admin_create_invite(
    State((pool, _settings)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Json(payload): Json<CreateInviteRequest>,
) -> Result<(StatusCode, Json<InviteResponse>), AppError> {
    let expires_hours = payload.expires_hours.unwrap_or(168).max(1).min(8760); // 1h–1y cap

    let invite = Invite::new(auth.user.user_id.clone(), payload.email, expires_hours);

    let invite_repo = InviteRepository::new(pool);
    invite_repo
        .create(&invite)
        .await
        .map_err(|e| AppError::Internal("Failed to create invite".to_string()))?;

    tracing::info!("Admin created invite: {}", invite.invite_id);

    Ok((StatusCode::CREATED, Json(invite_to_response(invite))))
}

/// GET /api/admin/invites
/// List all invites.
pub async fn admin_list_invites(
    State((pool, _settings)): State<AppState>,
    Extension(_auth): Extension<AuthMiddleware>,
    Query(query): Query<InviteListQuery>,
) -> Result<Json<InviteListResponse>, AppError> {
    let limit = query.limit.min(200);
    let invite_repo = InviteRepository::new(pool);
    let (invites, total) = invite_repo
        .list_all(limit, query.offset)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list invites: {:?}", e)))?;

    Ok(Json(InviteListResponse {
        invites: invites.into_iter().map(invite_to_response).collect(),
        total,
    }))
}

/// DELETE /api/admin/invites/:invite_id
/// Revoke (delete) an invite.
pub async fn admin_delete_invite(
    State((pool, _settings)): State<AppState>,
    Extension(_auth): Extension<AuthMiddleware>,
    Path(invite_id): Path<String>,
) -> Result<Json<MessageResponse>, AppError> {
    let invite_repo = InviteRepository::new(pool);

    if invite_repo
        .find_by_id(&invite_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to look up invite")))?
        .is_none()
    {
        return Err(AppError::NotFound("Invite not found".to_string()));
    }

    invite_repo
        .delete(&invite_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to delete invite")))?;

    Ok(Json(MessageResponse {
        message: "Invite revoked".to_string(),
    }))
}

// Email invite

/// POST /api/admin/invites/email
/// Create invite links and send them to a list of email addresses via SMTP.
pub async fn admin_send_email_invites(
    State((pool, settings)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Json(payload): Json<SendEmailInvitesRequest>,
) -> Result<Json<SendEmailInvitesResponse>, AppError> {
    // DB settings take priority; fall back to environment variables
    let email_repo = EmailSettingsRepository::new(pool.clone());
    let email_cfg = email_repo.get().await.map_err(|e| {
        tracing::error!("Failed to fetch email settings: {:?}", e);
        AppError::Internal("Failed to fetch email settings".to_string())
    })?;
    let use_db = smtp_is_configured_db(&email_cfg);

    if !use_db && !smtp_is_configured(&settings) {
        return Err(AppError::BadRequest(
            "SMTP is not configured. Configure it in Admin → Email.".to_string(),
        ));
    }
    if payload.emails.is_empty() {
        return Err(AppError::BadRequest(
            "No email addresses provided".to_string(),
        ));
    }

    let expires_hours = payload.expires_hours.unwrap_or(168).max(1).min(8760);
    let base_url = settings.base_url.trim_end_matches('/').to_string();

    // Fetch branding from theme settings
    let theme = ThemeRepository::new(pool.clone())
        .get_active()
        .await
        .ok()
        .flatten();
    let site_name = theme
        .as_ref()
        .map(|t| t.site_title.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("Magnolia");
    let accent = theme
        .as_ref()
        .map(|t| t.color_accent.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("#2563eb");

    let personal_message = payload.message.as_deref().filter(|s| !s.trim().is_empty());

    let invite_repo = InviteRepository::new(pool);
    let mut sent = Vec::new();
    let mut failed = Vec::new();

    for email in &payload.emails {
        let invite = Invite::new(
            auth.user.user_id.clone(),
            Some(email.clone()),
            expires_hours,
        );
        if let Err(e) = invite_repo.create(&invite).await {
            tracing::error!("Failed to create invite for {}: {:?}", email, e);
            failed.push(email.clone());
            continue;
        }

        let link = format!("{}/#register/{}", base_url, invite.token);
        let (subject, text_body, html_body) =
            build_invite_email(site_name, accent, &link, expires_hours, personal_message);

        let result = if use_db {
            send_email_db_html(&email_cfg, email, &subject, &text_body, &html_body).await
        } else {
            send_email_html(&settings, email, &subject, &text_body, &html_body).await
        };

        match result {
            Ok(_) => {
                tracing::info!("Invite email sent to {}", email);
                sent.push(email.clone());
            }
            Err(e) => {
                tracing::error!("Failed to send invite email to {}: {}", email, e);
                failed.push(email.clone());
            }
        }
    }

    Ok(Json(SendEmailInvitesResponse { sent, failed }))
}

// Registration application management

/// GET /api/admin/applications
pub async fn admin_list_applications(
    State((pool, _settings)): State<AppState>,
    Extension(_auth): Extension<AuthMiddleware>,
    Query(query): Query<ApplicationListQuery>,
) -> Result<Json<ApplicationListResponse>, AppError> {
    let limit = query.limit.min(200);
    let repo = RegisterApplicationRepository::new(pool);

    // Expire any stale pending applications before listing
    let _ = repo.expire_pending().await;

    let (apps, total) = repo
        .list(limit, query.offset, query.status.as_deref())
        .await
        .map_err(|e| {
            tracing::error!("Failed to list applications: {:?}", e);
            AppError::Internal("Failed to list applications".to_string())
        })?;

    Ok(Json(ApplicationListResponse {
        applications: apps.into_iter().map(app_to_response).collect(),
        total,
    }))
}

/// POST /api/admin/applications/:id/approve
/// Approve an application, create the user account, and send a setup link.
pub async fn admin_approve_application(
    State((pool, settings)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Path(application_id): Path<String>,
) -> Result<Json<ApproveApplicationResponse>, AppError> {
    let app_repo = RegisterApplicationRepository::new(pool.clone());
    let application = app_repo
        .find_by_id(&application_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Application not found".to_string()))?;

    if application.status != "pending" {
        return Err(AppError::BadRequest(format!(
            "Application is already '{}'",
            application.status
        )));
    }

    let user_repo = UserRepository::new(pool.clone());
    if user_repo.find_by_email(&application.email).await?.is_some() {
        return Err(AppError::Conflict(
            "An account with this email already exists".to_string(),
        ));
    }

    // Derive a default username from the email local-part; make it unique if needed.
    let base_username = application
        .email
        .split('@')
        .next()
        .unwrap_or("user")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>();
    let base_username = if base_username.len() < 3 {
        format!("user_{}", base_username)
    } else {
        base_username
    };
    let username = if user_repo.find_by_username(&base_username).await?.is_none() {
        base_username
    } else {
        format!(
            "{}_{}",
            base_username,
            &uuid::Uuid::new_v4().to_string()[..6]
        )
    };

    // Create the user with a random unusable password (they'll set their own via reset link)
    let dummy_hash = hash_password(&uuid::Uuid::new_v4().to_string())?;
    let mut user = UserAccount::new(username, Some(application.email.clone()), dummy_hash);
    if let Some(ref dn) = application.display_name {
        user.display_name = Some(dn.clone());
    }
    user.verified = 1; // Approved accounts skip email verification

    user_repo.create_user(&user).await?;

    // Create a password reset token so the user can set their password
    let reset = PasswordReset::new(user.user_id.clone());
    user_repo.create_password_reset_token(&reset).await?;

    let base_url = settings.base_url.trim_end_matches('/').to_string();
    let setup_link = format!("{}/#reset-password/{}", base_url, reset.token);

    // Mark application as approved
    app_repo
        .update_status(&application_id, "approved", &auth.user.user_id)
        .await?;

    tracing::info!(
        "Admin {} approved application {} — created user {}",
        auth.user.user_id,
        application_id,
        user.user_id
    );

    // Try to send setup email — DB settings take priority over env vars
    let email_repo = EmailSettingsRepository::new(pool.clone());
    let email_cfg = email_repo.get().await.ok();
    let use_db = email_cfg
        .as_ref()
        .map(|c| smtp_is_configured_db(c))
        .unwrap_or(false);

    let email_sent = if use_db || smtp_is_configured(&settings) {
        let subject = "Your Magnolia account has been approved";
        let body = format!(
            "Your registration application has been approved!\n\nClick the link below to set your password and log in:\n{}\n\nThis link expires in 1 hour.",
            setup_link
        );
        let result = if use_db {
            send_email_db(
                email_cfg.as_ref().unwrap(),
                &application.email,
                subject,
                &body,
            )
            .await
        } else {
            send_email(&settings, &application.email, subject, &body).await
        };
        match result {
            Ok(_) => {
                tracing::info!("Account setup email sent to {}", application.email);
                true
            }
            Err(e) => {
                tracing::error!("Failed to send setup email to {}: {}", application.email, e);
                false
            }
        }
    } else {
        false
    };

    Ok(Json(ApproveApplicationResponse {
        user_id: user.user_id,
        email: application.email,
        setup_link: if !email_sent { Some(setup_link) } else { None },
        email_sent,
    }))
}

/// POST /api/admin/applications/:id/deny
pub async fn admin_deny_application(
    State((pool, _settings)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Path(application_id): Path<String>,
) -> Result<Json<MessageResponse>, AppError> {
    let repo = RegisterApplicationRepository::new(pool);
    let application = repo
        .find_by_id(&application_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Application not found".to_string()))?;

    if application.status != "pending" {
        return Err(AppError::BadRequest(format!(
            "Application is already '{}'",
            application.status
        )));
    }

    repo.update_status(&application_id, "denied", &auth.user.user_id)
        .await?;

    tracing::info!(
        "Admin {} denied application {}",
        auth.user.user_id,
        application_id
    );

    Ok(Json(MessageResponse {
        message: "Application denied".to_string(),
    }))
}

/// DELETE /api/admin/applications/:id
pub async fn admin_delete_application(
    State((pool, _settings)): State<AppState>,
    Extension(_auth): Extension<AuthMiddleware>,
    Path(application_id): Path<String>,
) -> Result<Json<MessageResponse>, AppError> {
    let repo = RegisterApplicationRepository::new(pool);

    if repo.find_by_id(&application_id).await?.is_none() {
        return Err(AppError::NotFound("Application not found".to_string()));
    }

    repo.delete(&application_id).await?;

    Ok(Json(MessageResponse {
        message: "Application deleted".to_string(),
    }))
}

// Helpers

fn invite_to_response(invite: Invite) -> InviteResponse {
    InviteResponse {
        invite_id: invite.invite_id,
        token: invite.token,
        email: invite.email,
        created_by: invite.created_by,
        created_at: invite.created_at,
        expires_at: invite.expires_at,
        used_at: invite.used_at,
        used_by_user_id: invite.used_by_user_id,
    }
}

fn app_to_response(app: RegisterApplication) -> ApplicationResponse {
    let is_expired = app.is_expired();
    ApplicationResponse {
        application_id: app.application_id,
        email: app.email,
        display_name: app.display_name,
        message: app.message,
        status: app.status,
        created_at: app.created_at,
        expires_at: app.expires_at,
        reviewed_at: app.reviewed_at,
        reviewed_by: app.reviewed_by,
        is_expired,
    }
}

// ── STUN/TURN server management ──────────────────────────────────────────────

fn stun_to_response(s: StunServer) -> StunServerResponse {
    StunServerResponse {
        has_credential: s.credential.is_some(),
        id: s.id,
        url: s.url,
        username: s.username,
        enabled: s.enabled != 0,
        last_checked_at: s.last_checked_at,
        last_status: s.last_status,
        created_at: s.created_at,
        updated_at: s.updated_at,
    }
}

/// GET /api/admin/stun-servers
pub async fn admin_list_stun_servers(
    State((pool, _settings)): State<AppState>,
    Extension(_auth): Extension<AuthMiddleware>,
) -> Result<Json<Vec<StunServerResponse>>, AppError> {
    let repo = StunServerRepository::new(pool);
    let servers = repo.list_all().await?;
    Ok(Json(servers.into_iter().map(stun_to_response).collect()))
}

/// POST /api/admin/stun-servers
pub async fn admin_create_stun_server(
    State((pool, _settings)): State<AppState>,
    Extension(_auth): Extension<AuthMiddleware>,
    Json(payload): Json<CreateStunServerRequest>,
) -> Result<(StatusCode, Json<StunServerResponse>), AppError> {
    payload.validate().map_err(AppError::from)?;
    let now = chrono::Utc::now().to_rfc3339();
    let server = StunServer {
        id: uuid::Uuid::new_v4().to_string(),
        url: payload.url,
        username: payload.username,
        credential: payload.credential,
        enabled: if payload.enabled { 1 } else { 0 },
        last_checked_at: None,
        last_status: "unknown".to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    let repo = StunServerRepository::new(pool);
    repo.create(&server).await?;
    Ok((StatusCode::CREATED, Json(stun_to_response(server))))
}

/// PATCH /api/admin/stun-servers/:id
pub async fn admin_update_stun_server(
    State((pool, _settings)): State<AppState>,
    Extension(_auth): Extension<AuthMiddleware>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateStunServerRequest>,
) -> Result<Json<StunServerResponse>, AppError> {
    payload.validate().map_err(AppError::from)?;
    let repo = StunServerRepository::new(pool);
    let existing = repo
        .get_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("STUN server not found".to_string()))?;
    let now = chrono::Utc::now().to_rfc3339();
    let url = payload.url.as_deref().unwrap_or(&existing.url);
    let username = payload.username.as_deref().or(existing.username.as_deref());
    // credential: Some(v) = update, None = leave unchanged
    let credential = payload
        .credential
        .as_deref()
        .or(existing.credential.as_deref());
    let enabled = payload
        .enabled
        .map(|v| if v { 1 } else { 0 })
        .unwrap_or(existing.enabled);
    repo.update(&id, url, username, credential, enabled, &now)
        .await?;
    let updated = repo
        .get_by_id(&id)
        .await?
        .ok_or_else(|| AppError::Internal("STUN server missing after update".to_string()))?;
    Ok(Json(stun_to_response(updated)))
}

/// DELETE /api/admin/stun-servers/:id
pub async fn admin_delete_stun_server(
    State((pool, _settings)): State<AppState>,
    Extension(_auth): Extension<AuthMiddleware>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let repo = StunServerRepository::new(pool);
    repo.get_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound("STUN server not found".to_string()))?;
    repo.delete(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}
