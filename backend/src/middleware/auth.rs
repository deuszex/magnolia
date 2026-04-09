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
use crate::logging::events::SecurityEvent;
use crate::{security_log, security_warn};
use magnolia_common::errors::AppError;
use magnolia_common::models::UserAccount;
use magnolia_common::repositories::UserRepository;

const SESSION_COOKIE_NAME: &str = "session_id";

#[derive(Clone)]
pub struct AuthMiddleware {
    pub user: UserAccount,
}

pub async fn require_auth(
    State((pool, _settings)): State<(AnyPool, Arc<Settings>)>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    // Get session cookie
    let session_id = match jar.get(SESSION_COOKIE_NAME).map(|c| c.value().to_string()) {
        Some(id) => id,
        None => {
            security_warn!(
                SecurityEvent::UnauthorizedAccess,
                "Missing session cookie",
                endpoint = req.uri().path()
            );
            return Err(AppError::Unauthorized);
        }
    };

    // Validate session and get user
    let user_repo = UserRepository::new(pool);
    let session = match user_repo.find_session(&session_id).await? {
        Some(s) => s,
        None => {
            security_warn!(
                SecurityEvent::SessionInvalid,
                "Invalid session ID",
                session_id = session_id
            );
            return Err(AppError::Unauthorized);
        }
    };

    // Check expiration
    if session.is_expired() {
        user_repo.delete_session(&session_id).await?;
        security_log!(
            SecurityEvent::SessionExpired,
            "Session expired",
            session_id = session_id,
            user_id = session.user_id
        );
        return Err(AppError::Unauthorized);
    }

    // Get user
    let user = user_repo
        .find_by_id(&session.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    // Check if user is active
    if user.active == 0 {
        security_warn!(
            SecurityEvent::UnauthorizedAccess,
            "Inactive user attempted access",
            user_id = user.user_id,
            endpoint = req.uri().path()
        );
        return Err(AppError::Forbidden);
    }

    // Insert user into request extensions
    req.extensions_mut().insert(AuthMiddleware { user });

    Ok(next.run(req).await)
}

pub async fn require_admin(
    State((pool, _settings)): State<(AnyPool, Arc<Settings>)>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let endpoint = req.uri().path().to_string();

    // First check auth
    let session_id = match jar.get(SESSION_COOKIE_NAME).map(|c| c.value().to_string()) {
        Some(id) => id,
        None => {
            security_warn!(
                SecurityEvent::UnauthorizedAccess,
                "Admin endpoint accessed without session",
                endpoint = endpoint
            );
            return Err(AppError::Unauthorized);
        }
    };

    let user_repo = UserRepository::new(pool);
    let session = match user_repo.find_session(&session_id).await? {
        Some(s) => s,
        None => {
            security_warn!(
                SecurityEvent::SessionInvalid,
                "Invalid session on admin endpoint",
                session_id = session_id,
                endpoint = endpoint
            );
            return Err(AppError::Unauthorized);
        }
    };

    if session.is_expired() {
        user_repo.delete_session(&session_id).await?;
        security_log!(
            SecurityEvent::SessionExpired,
            "Session expired on admin endpoint",
            session_id = session_id,
            user_id = session.user_id
        );
        return Err(AppError::Unauthorized);
    }

    let user = user_repo
        .find_by_id(&session.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    // Check if user is active
    if user.active == 0 {
        security_warn!(
            SecurityEvent::AdminAccessDenied,
            "Inactive user attempted admin access",
            user_id = user.user_id,
            endpoint = endpoint
        );
        return Err(AppError::Forbidden);
    }

    // Check admin flag
    if user.admin == 0 {
        security_warn!(
            SecurityEvent::AdminAccessDenied,
            "Non-admin user attempted admin access",
            user_id = user.user_id,
            user_email = user.email.as_deref().unwrap_or(""),
            endpoint = endpoint
        );
        return Err(AppError::Forbidden);
    }

    security_log!(
        SecurityEvent::AdminAccessGranted,
        "Admin access granted",
        user_id = user.user_id,
        endpoint = endpoint
    );

    req.extensions_mut().insert(AuthMiddleware { user });

    Ok(next.run(req).await)
}

// Extract user from request extensions
pub trait RequireAuth {
    fn user(&self) -> Result<&UserAccount, AppError>;
}

impl RequireAuth for Request {
    fn user(&self) -> Result<&UserAccount, AppError> {
        self.extensions()
            .get::<AuthMiddleware>()
            .map(|auth| &auth.user)
            .ok_or(AppError::Unauthorized)
    }
}

// Optional auth extractor - returns Some(AuthMiddleware) if authenticated, None otherwise
pub struct OptionalAuth(pub Option<AuthMiddleware>);

impl<S> axum::extract::FromRequestParts<S> for OptionalAuth
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        // Try to get auth from extensions (will be there if middleware ran)
        let auth = parts.extensions.get::<AuthMiddleware>().cloned();
        Ok(OptionalAuth(auth))
    }
}
