use axum::{
    Router, middleware,
    routing::{delete, get, post, put},
};

use super::handlers;
use super::handlers::AppState;
use crate::middleware::auth::{require_admin, require_auth};
use crate::middleware::rate_limit::{RateLimiter, create_auth_rate_limit_middleware};

/// Build the federation sub-router. Merged into the main router in routes.rs.
/// `s2s_limiter`: rate limit for general S2S endpoints.
/// `s2s_media_limiter`: separate tighter rate limit for media streaming (file transfers are heavy).
pub fn federation_router(
    state: AppState,
    s2s_limiter: RateLimiter,
    s2s_media_limiter: RateLimiter,
) -> Router<AppState> {
    let admin_routes = Router::new()
        // Settings
        .route(
            "/api/admin/federation/settings",
            get(handlers::admin_get_federation_settings)
                .put(handlers::admin_update_federation_settings),
        )
        // Connection management
        .route(
            "/api/admin/federation/connections",
            get(handlers::admin_list_connections).post(handlers::admin_initiate_connection),
        )
        .route(
            "/api/admin/federation/connections/{id}",
            get(handlers::admin_get_connection)
                .put(handlers::admin_update_connection)
                .delete(handlers::admin_revoke_connection),
        )
        .route(
            "/api/admin/federation/connections/{id}/accept",
            post(handlers::admin_accept_connection),
        )
        .route(
            "/api/admin/federation/connections/{id}/reject",
            post(handlers::admin_reject_connection),
        )
        // Discovery hints
        .route(
            "/api/admin/federation/discovery",
            get(handlers::admin_list_discovery),
        )
        .route(
            "/api/admin/federation/discovery/{id}",
            delete(handlers::admin_dismiss_discovery),
        )
        // Hub WS status
        .route(
            "/api/admin/federation/hub-status",
            get(handlers::admin_get_hub_status),
        )
        .layer(middleware::from_fn_with_state(state.clone(), require_admin));

    // S2S media endpoint — separate rate limiter (file transfers are expensive).
    let s2s_media_routes = Router::new()
        .route(
            "/api/s2s/media/{media_id}",
            get(handlers::s2s_serve_media_file),
        )
        .layer(middleware::from_fn(create_auth_rate_limit_middleware(
            s2s_media_limiter,
        )));

    // S2S inbound endpoints — no session auth, verified via ML-DSA signature.
    // Rate-limited per source IP to bound flood attacks from a single peer.
    let s2s_routes = Router::new()
        .route("/api/s2s/connect", post(handlers::s2s_receive_connect))
        .route(
            "/api/s2s/connect/accept",
            post(handlers::s2s_receive_accept),
        )
        .route(
            "/api/s2s/connect/reject",
            post(handlers::s2s_receive_reject),
        )
        .route(
            "/api/s2s/disconnect",
            post(handlers::s2s_receive_disconnect),
        )
        .route("/api/s2s/users/sync", post(handlers::s2s_receive_user_sync))
        .route("/api/s2s/message", post(handlers::s2s_receive_message))
        .route(
            "/api/s2s/call-signal",
            post(handlers::s2s_receive_call_signal),
        )
        .route("/api/s2s/discovery", post(handlers::s2s_receive_discovery))
        .route("/api/s2s/posts", post(handlers::s2s_serve_posts))
        .route("/api/s2s/ws", get(handlers::s2s_ws_upgrade))
        .route(
            "/api/s2s/users/{user_id}/ecdh-key",
            get(handlers::s2s_get_ecdh_key),
        )
        .route(
            "/api/s2s/users/identity/{username}",
            get(handlers::s2s_get_user_identity),
        )
        .layer(middleware::from_fn(create_auth_rate_limit_middleware(
            s2s_limiter,
        )));

    // User-facing federation endpoints.
    let user_routes = Router::new()
        .route("/api/federation/users", get(handlers::search_remote_users))
        .route(
            "/api/federation/servers",
            get(handlers::list_servers_for_user),
        )
        .route("/api/federation/dm", post(handlers::start_federated_dm))
        .route(
            "/api/users/federation-settings",
            get(handlers::get_my_federation_settings).put(handlers::update_my_federation_settings),
        )
        .route(
            "/api/users/federation-bans",
            get(handlers::list_external_bans).post(handlers::add_external_ban),
        )
        .route(
            "/api/users/federation-bans/{server_id}/{remote_user_id}",
            delete(handlers::remove_external_ban),
        )
        .route(
            "/api/users/federation-rules",
            get(handlers::list_my_federation_rules).post(handlers::add_my_federation_rule),
        )
        .route(
            "/api/users/federation-rules/{server_id}/{rule_type}",
            delete(handlers::remove_my_federation_rule),
        )
        .route("/api/posts/federated", get(handlers::get_federated_feed))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .merge(admin_routes)
        .merge(s2s_media_routes)
        .merge(s2s_routes)
        .merge(user_routes)
}
