use axum::{
    Extension, Router,
    extract::DefaultBodyLimit,
    http::{Method, header},
    middleware,
    routing::{delete, get, patch, post, put},
};
use sqlx::AnyPool;
use std::{env, sync::Arc};
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;

use crate::federation;
use crate::handlers;
use crate::handlers::{
    admin as admin_handlers, auth as auth_handlers, calling as calling_handlers,
    comment as comment_handlers, events as event_handlers, global_call as global_call_handlers,
    link_preview as link_preview_handlers, media as media_handlers,
    messaging as messaging_handlers, post as post_handlers, setup as setup_handlers,
    tag as tag_handlers, theme as theme_handlers, ws as ws_handlers,
};
use crate::middleware::auth::{require_admin, require_auth};
use crate::middleware::rate_limit::{RateLimiter, create_auth_rate_limit_middleware};
use crate::middleware::security_audit::{AuditService, audit_middleware};
use crate::{config::Settings, middleware::security_headers::add_security_headers};

pub type AppState = (AnyPool, Arc<Settings>);

pub fn create_router(
    pool: AnyPool,
    settings: Arc<Settings>,
    audit_service: Option<AuditService>,
    setup_required: bool,
    identity: Arc<federation::identity::ServerIdentity>,
    s2s_client: federation::client::S2SClient,
    hub_registry: federation::hub::HubRegistry,
    hub_status: federation::hub::PeerStatusMap,
) -> Router {
    let state: AppState = (pool.clone(), settings.clone());

    // WebSocket connection registry (shared between WS and calling routes)
    let registry = ws_handlers::new_registry();
    ws_handlers::init_global_registry(registry.clone());

    // Rate limiters
    let trusted_proxy = settings.trusted_proxy.clone();
    let auth_limiter = RateLimiter::new(5, 60, trusted_proxy.clone());
    let search_limiter = RateLimiter::new(30, 60, trusted_proxy.clone());
    let upload_limiter = RateLimiter::new(20, 60, trusted_proxy.clone());
    let ws_limiter = RateLimiter::new(10, 60, trusted_proxy.clone());
    // Public content: generous limit to allow normal browsing while blocking scrapers
    let public_limiter = RateLimiter::new(120, 60, trusted_proxy.clone());
    // Link preview makes outbound HTTP requests — tighter limit per authenticated user
    let link_preview_limiter = RateLimiter::new(20, 60, trusted_proxy.clone());
    // Change-password: sensitive operation, same tight limit as login
    let change_password_limiter = RateLimiter::new(5, 60, trusted_proxy.clone());
    // S2S inbound: generous per-peer limit since a peer may send many messages,
    // but still bounds flood attacks from a single origin IP
    let s2s_limiter = RateLimiter::new(60, 60, trusted_proxy.clone());
    // S2S media: tighter limit — file transfers are expensive
    let s2s_media_limiter = RateLimiter::new(30, 60, trusted_proxy);

    // Auth routes with strict rate limiting and optional audit logging
    let mut auth_routes = Router::new()
        .route("/api/auth/register", post(handlers::register))
        .route("/api/auth/apply", post(auth_handlers::submit_application))
        .route("/api/auth/config", get(auth_handlers::get_auth_config))
        .route("/api/auth/verify-email", post(handlers::verify_email))
        .route(
            "/api/auth/resend-verification",
            post(handlers::resend_verification),
        )
        .route("/api/auth/login", post(handlers::login))
        // Password reset endpoints
        .route(
            "/api/auth/request-password-reset",
            post(handlers::request_password_reset),
        )
        .route(
            "/api/auth/validate-password-reset",
            post(handlers::validate_password_reset),
        )
        .route("/api/auth/reset-password", post(handlers::reset_password))
        .route("/api/auth/logout", post(auth_handlers::logout))
        // Auth payloads are tiny (email + password). A tight body limit prevents
        // large-body DoS before the rate limiter even runs.
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn(create_auth_rate_limit_middleware(
            auth_limiter,
        )));

    // Add audit middleware to auth routes if enabled
    if let Some(ref service) = audit_service {
        auth_routes = auth_routes
            .layer(Extension(service.clone()))
            .layer(middleware::from_fn(audit_middleware));
    }

    // Search with rate limiting (prevents DoS/scraping)
    let search_routes = Router::new()
        .route("/api/posts/search", get(post_handlers::search_posts))
        .route("/api/tags", get(tag_handlers::list_tags))
        .layer(middleware::from_fn(create_auth_rate_limit_middleware(
            search_limiter,
        )));

    // Public routes (no authentication required)
    let mut public_routes = Router::new()
        // Health check
        .route("/health", get(health_check))
        // Public theme (for CSS variable injection on load)
        .route("/api/theme", get(theme_handlers::get_theme))
        // Merge auth routes with rate limiting
        .merge(auth_routes)
        // Merge search routes with rate limiting
        .merge(search_routes);

    // Setup routes are only registered when no users exist yet.
    // Once setup is complete the endpoints return 404 rather than 403,
    // giving no information that setup was ever available.
    if setup_required {
        public_routes = public_routes
            .route("/api/setup/status", get(setup_handlers::setup_status))
            .route("/api/setup", post(setup_handlers::setup));
    }

    // Post routes (public read, authenticated write)
    let post_routes = Router::new()
        // Public: list and view published posts
        .route("/api/posts", get(post_handlers::list_posts))
        .route("/api/posts/{post_id}", get(post_handlers::get_post))
        .layer(middleware::from_fn(create_auth_rate_limit_middleware(
            public_limiter.clone(),
        )));

    // Protected post routes (authentication required)
    let protected_post_routes = Router::new()
        .route("/api/posts", post(post_handlers::create_post))
        .route("/api/posts/{post_id}", put(post_handlers::update_post))
        .route("/api/posts/{post_id}", delete(post_handlers::delete_post))
        .route(
            "/api/posts/{post_id}/publish",
            post(post_handlers::toggle_publish),
        )
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Comment routes
    let comment_routes = Router::new()
        // Public: view comments
        .route(
            "/api/posts/{post_id}/comments",
            get(comment_handlers::list_comments),
        )
        .route(
            "/api/posts/{post_id}/comments/count",
            get(comment_handlers::get_comment_count),
        )
        .route(
            "/api/comments/{comment_id}",
            get(comment_handlers::get_comment),
        )
        .route(
            "/api/comments/{comment_id}/replies",
            get(comment_handlers::list_replies),
        )
        .layer(middleware::from_fn(create_auth_rate_limit_middleware(
            public_limiter,
        )));

    // Protected comment routes
    let protected_comment_routes = Router::new()
        .route(
            "/api/posts/{post_id}/comments",
            post(comment_handlers::create_comment),
        )
        .route(
            "/api/comments/{comment_id}",
            put(comment_handlers::update_comment),
        )
        .route(
            "/api/comments/{comment_id}",
            delete(comment_handlers::delete_comment),
        )
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Media read/mutation routes (all protected)
    let media_routes = Router::new()
        .route("/api/media", get(media_handlers::list_media))
        .route("/api/media/images", get(media_handlers::list_images))
        .route("/api/media/videos", get(media_handlers::list_videos))
        .route("/api/media/files", get(media_handlers::list_files))
        .route("/api/media/storage", get(media_handlers::get_storage_usage))
        .route(
            "/api/media/batch-delete",
            post(media_handlers::batch_delete_media),
        )
        .route("/api/media/{media_id}", get(media_handlers::get_media))
        .route("/api/media/{media_id}", put(media_handlers::update_media))
        .route(
            "/api/media/{media_id}",
            delete(media_handlers::delete_media),
        )
        .route(
            "/api/media/{media_id}/file",
            get(media_handlers::serve_media_file),
        )
        .route(
            "/api/media/{media_id}/thumbnail",
            get(media_handlers::serve_thumbnail),
        )
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Upload routes — rate limited separately to prevent disk exhaustion
    let upload_routes = Router::new()
        .route("/api/media", post(media_handlers::upload_media))
        .route(
            "/api/media/chunked/init",
            post(media_handlers::init_chunked_upload),
        )
        .route(
            "/api/media/chunked/{upload_id}/{chunk_number}",
            post(media_handlers::upload_chunk),
        )
        .route(
            "/api/media/chunked/{upload_id}/complete",
            post(media_handlers::complete_chunked_upload),
        )
        .layer(middleware::from_fn(create_auth_rate_limit_middleware(
            upload_limiter,
        )))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Event routes (all protected, registry needed for WS push)
    let event_routes = Router::new()
        .route("/api/events", get(event_handlers::list_events))
        .route("/api/events/count", get(event_handlers::get_unread_count))
        .route(
            "/api/events/viewed-all",
            put(event_handlers::mark_all_events_viewed),
        )
        .route(
            "/api/events/prefs",
            get(event_handlers::get_event_prefs).put(event_handlers::update_event_prefs),
        )
        .route(
            "/api/events/{id}/viewed",
            put(event_handlers::mark_event_viewed),
        )
        .route("/api/events/{id}", delete(event_handlers::delete_event))
        .route(
            "/api/profile/email-visible",
            put(event_handlers::update_email_visible),
        )
        .layer(Extension(registry.clone()))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Link preview (authenticated — prevents open-proxy abuse; rate-limited — makes outbound HTTP)
    let link_preview_routes = Router::new()
        .route(
            "/api/link-preview",
            get(link_preview_handlers::get_link_preview),
        )
        .layer(middleware::from_fn(create_auth_rate_limit_middleware(
            link_preview_limiter,
        )))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Protected auth routes
    let protected_auth_routes = Router::new()
        .route("/api/auth/me", get(auth_handlers::get_current_user))
        .route("/api/users", get(auth_handlers::list_users))
        .route(
            "/api/users/{user_id}/profile",
            get(auth_handlers::get_profile),
        )
        .route("/api/profile", put(auth_handlers::update_profile))
        .route(
            "/api/auth/me/public-key",
            put(auth_handlers::update_public_key),
        )
        .route(
            "/api/auth/me/e2e-key",
            get(auth_handlers::get_e2e_key_blob).put(auth_handlers::set_e2e_key_blob),
        )
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Change-password: auth required + tight rate limit (same as login)
    let change_password_routes = Router::new()
        .route(
            "/api/auth/change-password",
            post(auth_handlers::change_password),
        )
        .layer(middleware::from_fn(create_auth_rate_limit_middleware(
            change_password_limiter,
        )))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Admin routes (authentication + admin flag required)
    let admin_routes = Router::new()
        // Site configuration
        .route(
            "/api/admin/site-config",
            get(admin_handlers::get_site_config),
        )
        .route(
            "/api/admin/site-config",
            put(admin_handlers::update_site_config),
        )
        // User management
        .route("/api/admin/users", get(admin_handlers::admin_list_users))
        .route("/api/admin/users", post(admin_handlers::admin_create_user))
        .route(
            "/api/admin/users/{user_id}",
            delete(admin_handlers::admin_delete_user),
        )
        .route(
            "/api/admin/users/{user_id}",
            patch(admin_handlers::admin_update_user),
        )
        // Invite management
        .route(
            "/api/admin/invites",
            post(admin_handlers::admin_create_invite),
        )
        .route(
            "/api/admin/invites",
            get(admin_handlers::admin_list_invites),
        )
        .route(
            "/api/admin/invites/{invite_id}",
            delete(admin_handlers::admin_delete_invite),
        )
        .route(
            "/api/admin/invites/email",
            post(admin_handlers::admin_send_email_invites),
        )
        // Registration applications
        .route(
            "/api/admin/applications",
            get(admin_handlers::admin_list_applications),
        )
        .route(
            "/api/admin/applications/{id}/approve",
            post(admin_handlers::admin_approve_application),
        )
        .route(
            "/api/admin/applications/{id}/deny",
            post(admin_handlers::admin_deny_application),
        )
        .route(
            "/api/admin/applications/{id}",
            delete(admin_handlers::admin_delete_application),
        )
        // Theme
        .route("/api/admin/theme", put(theme_handlers::admin_update_theme))
        // Email settings
        .route(
            "/api/admin/email-settings",
            get(admin_handlers::get_email_settings),
        )
        .route(
            "/api/admin/email-settings",
            put(admin_handlers::update_email_settings),
        )
        // STUN/TURN server management
        .route(
            "/api/admin/stun-servers",
            get(admin_handlers::admin_list_stun_servers)
                .post(admin_handlers::admin_create_stun_server),
        )
        .route(
            "/api/admin/stun-servers/{id}",
            patch(admin_handlers::admin_update_stun_server)
                .delete(admin_handlers::admin_delete_stun_server),
        )
        .layer(middleware::from_fn_with_state(state.clone(), require_admin));

    // Add audit middleware to admin routes if enabled
    let admin_routes = if let Some(ref service) = audit_service {
        admin_routes
            .layer(Extension(service.clone()))
            .layer(middleware::from_fn(audit_middleware))
    } else {
        admin_routes
    };

    // Messaging routes (all protected)
    let messaging_routes = Router::new()
        .route(
            "/api/messaging/preferences",
            get(messaging_handlers::get_preferences),
        )
        .route(
            "/api/messaging/preferences",
            put(messaging_handlers::update_preferences),
        )
        .route(
            "/api/messaging/blacklist",
            get(messaging_handlers::list_blocks),
        )
        .route(
            "/api/messaging/blacklist",
            post(messaging_handlers::create_block),
        )
        .route(
            "/api/messaging/blacklist/{user_id}",
            delete(messaging_handlers::delete_block),
        )
        .route(
            "/api/conversations",
            post(messaging_handlers::create_conversation),
        )
        .route(
            "/api/conversations",
            get(messaging_handlers::list_conversations),
        )
        .route(
            "/api/conversations/{id}",
            get(messaging_handlers::get_conversation),
        )
        .route(
            "/api/conversations/{id}",
            put(messaging_handlers::update_conversation),
        )
        .route(
            "/api/conversations/{id}",
            delete(messaging_handlers::delete_conversation),
        )
        .route(
            "/api/conversations/{id}/members",
            post(messaging_handlers::add_member),
        )
        .route(
            "/api/conversations/{id}/members/{user_id}",
            delete(messaging_handlers::remove_member),
        )
        .route(
            "/api/conversations/{id}/messages",
            post(messaging_handlers::send_message),
        )
        .route(
            "/api/conversations/{id}/messages",
            get(messaging_handlers::list_messages),
        )
        .route(
            "/api/messages/{id}",
            delete(messaging_handlers::delete_message),
        )
        .route(
            "/api/messaging/favourites",
            post(messaging_handlers::add_favourite),
        )
        .route(
            "/api/messaging/favourites/{conversation_id}",
            delete(messaging_handlers::remove_favourite),
        )
        .route(
            "/api/messaging/unread",
            get(messaging_handlers::get_unread_counts),
        )
        .route(
            "/api/conversations/{id}/media",
            get(messaging_handlers::get_conversation_media),
        )
        .route(
            "/api/conversations/{id}/background",
            get(messaging_handlers::get_background),
        )
        .route(
            "/api/conversations/{id}/background",
            put(messaging_handlers::set_background),
        )
        .route(
            "/api/conversations/{id}/background",
            delete(messaging_handlers::delete_background),
        )
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // WebSocket route — rate limited + auth-gated.
    // The handler also validates the session before upgrading (defence in depth).
    let ws_routes = Router::new()
        .route("/api/ws", get(ws_handlers::ws_handler))
        .layer(Extension(registry.clone()))
        .layer(middleware::from_fn(create_auth_rate_limit_middleware(
            ws_limiter,
        )))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Calling routes (protected)
    let calling_routes = Router::new()
        .route(
            "/api/calls/ice-config",
            get(calling_handlers::get_ice_config),
        )
        .route(
            "/api/calls/history",
            get(calling_handlers::list_call_history),
        )
        .route(
            "/api/conversations/{id}/calls",
            get(calling_handlers::list_conversation_calls),
        )
        .route(
            "/api/conversations/{id}/calls",
            post(calling_handlers::initiate_call),
        )
        .route(
            "/api/conversations/{id}/active-call",
            get(calling_handlers::get_active_call),
        )
        .layer(Extension(registry.clone()))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Global call routes (protected)
    let global_call_routes = Router::new()
        .route(
            "/api/global-call",
            get(global_call_handlers::get_global_call),
        )
        .route(
            "/api/global-call/join",
            post(global_call_handlers::join_global_call),
        )
        .route(
            "/api/global-call/leave",
            post(global_call_handlers::leave_global_call),
        )
        .layer(Extension(registry.clone()))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // CORS layer - proper configuration

    let origins = [env::var("WEB_ORIGIN").unwrap().parse().unwrap()];
    let cors = CorsLayer::new()
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
        ])
        .allow_origin(origins)
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    // Federation routes (admin + S2S inbound + user-facing)
    // Registry is needed so inbound S2S messages can notify local users via WebSocket.
    let fed_routes =
        federation::routes::federation_router(state.clone(), s2s_limiter, s2s_media_limiter)
            .layer(Extension(registry.clone()));

    // Combine all routes
    Router::new()
        .merge(public_routes)
        // Post routes
        .merge(post_routes)
        .merge(protected_post_routes)
        // Comment routes
        .merge(comment_routes)
        .merge(protected_comment_routes)
        // Media routes
        .merge(media_routes)
        .merge(upload_routes)
        // Protected auth routes
        .merge(protected_auth_routes)
        .merge(change_password_routes)
        .merge(admin_routes)
        // Messaging routes
        .merge(messaging_routes)
        // WebSocket signaling
        .merge(ws_routes)
        // Calling routes
        .merge(calling_routes)
        // Global call routes
        .merge(global_call_routes)
        // Events
        .merge(event_routes)
        // Link preview
        .merge(link_preview_routes)
        // Federation
        .merge(fed_routes)
        // Static files
        // Admin-only: these files expose admin UI — block non-admins entirely.
        .merge(
            Router::new()
                .route("/js/admin.js", get(handlers::serve_js_admin_file))
                .route(
                    "/locales/admin.json",
                    get(handlers::serve_locale_admin_file),
                )
                .layer(middleware::from_fn_with_state(state.clone(), require_admin)),
        )
        // All other JS is public — security is enforced by the API, not by hiding JS.
        .route("/js/api.js", get(handlers::serve_js_api_file))
        .route("/js/auth.js", get(handlers::serve_js_auth_page))
        .route("/js/main.js", get(handlers::serve_js_main_file))
        .route("/js/{*path}", get(handlers::serve_embedded_js))
        // CSS, assets, and locales are public (needed for the login page).
        // admin.json locale is already handled above.
        .route("/css/{*path}", get(handlers::serve_embedded_css))
        .route("/assets/{*path}", get(handlers::serve_embedded_assets))
        .route("/locales/{filename}", get(handlers::serve_embedded_locale))
        // Favicon from embedded assets
        .route("/favicon.ico", get(handlers::serve_favicon))
        .layer(Extension(s2s_client))
        .layer(Extension(identity))
        .layer(Extension(hub_registry))
        .layer(Extension(hub_status))
        .layer(middleware::from_fn(add_security_headers))
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024)) // 50MB limit for image uploads
        .layer(CompressionLayer::new()) // Gzip compression for responses
        .layer(cors)
        // Fallback to serve appropriate HTML based on auth
        .fallback(handlers::serve_app)
        .with_state(state)
}

async fn health_check() -> &'static str {
    "OK"
}
