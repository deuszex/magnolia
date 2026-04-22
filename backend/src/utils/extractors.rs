use axum::extract::FromRequestParts;
use axum::{extract::ConnectInfo, http::request::Parts};
use std::convert::Infallible;
use std::net::SocketAddr;

/// Extracts the client IP address from `X-Forwarded-For` / `X-Real-IP` headers,
/// falling back to the raw socket address from `ConnectInfo` if available.
/// Always succeeds (rejection is Infallible).
pub struct ClientIp(pub Option<String>);

impl<S> FromRequestParts<S> for ClientIp
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Prefer forwarded headers (set by reverse proxy)
        let ip = parts
            .headers
            .get("x-forwarded-for")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.split(',').next())
            .map(|s| s.trim().to_string())
            .or_else(|| {
                parts
                    .headers
                    .get("x-real-ip")
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.to_string())
            })
            // Fall back to socket addr from ConnectInfo extension
            .or_else(|| {
                parts
                    .extensions
                    .get::<ConnectInfo<SocketAddr>>()
                    .map(|ci| ci.0.ip().to_string())
            });

        Ok(ClientIp(ip))
    }
}
