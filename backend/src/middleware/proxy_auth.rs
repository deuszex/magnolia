use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use axum_extra::extract::CookieJar;
use sqlx::AnyPool;
use std::sync::Arc;

use crate::config::Settings;
use magnolia_common::errors::AppError;
use magnolia_common::models::ProxyUserAccount;
use magnolia_common::repositories::ProxyUserRepository;

const PROXY_SESSION_COOKIE: &str = "proxy_session_id";

/// Injected into request extensions when a valid proxy session cookie is present.
#[derive(Clone)]
pub struct ProxyAuthMiddleware {
    pub proxy: ProxyUserAccount,
}

pub async fn require_proxy_auth(
    State((pool, _settings)): State<(AnyPool, Arc<Settings>)>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let session_id = jar
        .get(PROXY_SESSION_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;

    let repo = ProxyUserRepository::new(pool);

    let session = repo
        .find_session(&session_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if session.is_expired() {
        let _ = repo.delete_session(&session_id).await;
        return Err(AppError::Unauthorized);
    }

    let proxy = repo
        .find_by_id(&session.proxy_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if proxy.active == 0 {
        return Err(AppError::Forbidden);
    }

    req.extensions_mut().insert(ProxyAuthMiddleware { proxy });

    Ok(next.run(req).await)
}
