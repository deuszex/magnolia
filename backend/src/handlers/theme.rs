//! Theme handlers — public theme read + admin theme update

use axum::{Extension, Json, extract::State};
use sqlx::AnyPool;
use std::sync::Arc;
use validator::Validate;

use crate::config::Settings;
use crate::middleware::auth::AuthMiddleware;
use magnolia_common::errors::AppError;
use magnolia_common::models::ThemeResponse;
use magnolia_common::repositories::ThemeRepository;
use magnolia_common::schemas::UpdateThemeRequest;

type AppState = (AnyPool, Arc<Settings>);

/// GET /api/theme
/// Public endpoint — returns the active theme so the frontend can apply CSS variables.
pub async fn get_theme(
    State((pool, _settings)): State<AppState>,
) -> Result<Json<ThemeResponse>, AppError> {
    let repo = ThemeRepository::new(pool);
    let theme = repo.get_active().await.map_err(|e| {
        tracing::error!("Failed to fetch theme: {:?}", e);
        AppError::Internal("Failed to fetch theme".to_string())
    })?;

    let response = match theme {
        Some(t) => ThemeResponse::from(t),
        None => default_theme_response(),
    };

    Ok(Json(response))
}

/// PUT /api/admin/theme
/// Admin only — update the active theme settings.
pub async fn admin_update_theme(
    State((pool, _settings)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Json(payload): Json<UpdateThemeRequest>,
) -> Result<Json<ThemeResponse>, AppError> {
    payload.validate().map_err(AppError::from)?;

    let repo = ThemeRepository::new(pool);
    let updated = repo
        .update(
            &payload.site_style,
            &payload.color_background,
            &payload.color_main,
            &payload.color_accent,
            &payload.color_button,
            &payload.color_button_hover,
            &payload.color_status_ready,
            &payload.color_status_pending,
            &payload.color_status_removed,
            payload.background_image.as_deref(),
            payload.banner_top_left_image.as_deref(),
            payload.banner_top_left_link.as_deref(),
            payload.banner_bottom_left_image.as_deref(),
            payload.banner_bottom_left_link.as_deref(),
            payload.banner_top_right_image.as_deref(),
            payload.banner_top_right_link.as_deref(),
            payload.banner_bottom_right_image.as_deref(),
            payload.banner_bottom_right_link.as_deref(),
            &payload.site_title,
            &payload.brand_icon,
            &payload.brand_text,
            payload.favicon_data.as_deref(),
            &payload.hero_title,
            payload.hero_subtitle.as_deref(),
            &auth.user.user_id,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to update theme: {:?}", e);
            AppError::Internal("Failed to update theme".to_string())
        })?;

    tracing::info!("Admin {} updated site theme", auth.user.user_id);

    Ok(Json(ThemeResponse::from(updated)))
}

fn default_theme_response() -> ThemeResponse {
    ThemeResponse {
        site_style: "glassmorphism".to_string(),
        color_background: "#080d1a".to_string(),
        color_main: "#e2e8f0".to_string(),
        color_accent: "#6366f1".to_string(),
        color_button: "#6366f1".to_string(),
        color_button_hover: "#4f46e5".to_string(),
        color_status_ready: "#22c55e".to_string(),
        color_status_pending: "#f59e0b".to_string(),
        color_status_removed: "#ef4444".to_string(),
        background_image: None,
        banner_top_left_image: None,
        banner_top_left_link: None,
        banner_bottom_left_image: None,
        banner_bottom_left_link: None,
        banner_top_right_image: None,
        banner_top_right_link: None,
        banner_bottom_right_image: None,
        banner_bottom_right_link: None,
        site_title: "Magnolia".to_string(),
        brand_icon: "❦".to_string(),
        brand_text: "Magnolia".to_string(),
        favicon_data: None,
        hero_title: "Welcome".to_string(),
        hero_subtitle: None,
    }
}
