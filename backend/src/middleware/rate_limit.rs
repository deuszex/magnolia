use axum::{
    extract::{ConnectInfo, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::logging::events::SecurityEvent;
use crate::security_warn;

/// Simple in-memory rate limiter
/// For production, consider using Redis or a distributed solution
#[derive(Clone)]
pub struct RateLimiter {
    // IP -> (request_count, window_start)
    store: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
    max_requests: u32,
    window_duration: Duration,
    /// Only trust X-Forwarded-For / X-Real-IP headers when the connection
    /// originates from this IP (i.e. a known reverse proxy).
    trusted_proxy: Option<String>,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_seconds: u64, trusted_proxy: Option<String>) -> Self {
        Self {
            store: Arc::new(Mutex::new(HashMap::new())),
            max_requests,
            window_duration: Duration::from_secs(window_seconds),
            trusted_proxy,
        }
    }

    pub async fn check_rate_limit(&self, ip: &str) -> Result<(), ()> {
        let mut store = self.store.lock().await;
        let now = Instant::now();

        let entry = store.entry(ip.to_string()).or_insert((0, now));

        // Check if window has expired
        if now.duration_since(entry.1) > self.window_duration {
            // Reset window
            entry.0 = 1;
            entry.1 = now;
            return Ok(());
        }

        // Increment counter
        entry.0 += 1;

        if entry.0 > self.max_requests {
            Err(())
        } else {
            Ok(())
        }
    }

    /// Cleanup old entries periodically
    pub async fn cleanup(&self) {
        let mut store = self.store.lock().await;
        let now = Instant::now();
        store
            .retain(|_, (_, timestamp)| now.duration_since(*timestamp) <= self.window_duration * 2);
    }
}

/// Extract the client IP address from a request.
///
/// The real TCP connection IP is always preferred. Forwarded headers are only
/// trusted when the connection comes from the configured trusted proxy IP,
/// preventing clients from spoofing their address to bypass rate limiting.
fn get_client_ip(req: &Request, trusted_proxy: Option<&str>) -> String {
    let conn_ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().to_string());

    let from_trusted_proxy = trusted_proxy.is_some_and(|proxy| conn_ip.as_deref() == Some(proxy));

    if from_trusted_proxy {
        if let Some(forwarded) = req.headers().get("x-forwarded-for") {
            if let Ok(s) = forwarded.to_str() {
                if let Some(ip) = s.split(',').next() {
                    return ip.trim().to_string();
                }
            }
        }
        if let Some(real_ip) = req.headers().get("x-real-ip") {
            if let Ok(s) = real_ip.to_str() {
                return s.to_string();
            }
        }
    }

    conn_ip.unwrap_or_else(|| "unknown".to_string())
}

/// Middleware for rate limiting
pub async fn rate_limit_middleware(
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    // For rate limiting, we need to extract from request extensions
    // The limiter should be added as an extension in the router
    let endpoint = req.uri().path().to_string();

    // Get limiter from extensions
    if let Some(limiter) = req.extensions().get::<RateLimiter>().cloned() {
        let ip = get_client_ip(&req, limiter.trusted_proxy.as_deref());
        match limiter.check_rate_limit(&ip).await {
            Ok(_) => Ok(next.run(req).await),
            Err(_) => {
                security_warn!(
                    SecurityEvent::RateLimitExceeded,
                    "Rate limit exceeded",
                    source_ip = ip,
                    endpoint = endpoint
                );
                Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    "Rate limit exceeded. Please try again later.".to_string(),
                ))
            }
        }
    } else {
        // No limiter configured, pass through
        Ok(next.run(req).await)
    }
}

/// Create middleware for auth rate limiting
pub fn create_auth_rate_limit_middleware(
    limiter: RateLimiter,
) -> impl Fn(
    Request,
    Next,
)
    -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, StatusCode>> + Send>>
+ Clone {
    move |req: Request, next: Next| {
        let limiter = limiter.clone();
        Box::pin(async move {
            let ip = get_client_ip(&req, limiter.trusted_proxy.as_deref());
            let endpoint = req.uri().path().to_string();

            match limiter.check_rate_limit(&ip).await {
                Ok(_) => Ok(next.run(req).await),
                Err(_) => {
                    tracing::warn!(
                    target: "security",
                    event_category = "intrusion_detection",
                    event_action = "rate_limit_exceeded",
                    event_outcome = "failure",
                    source_ip = %ip,
                    endpoint = %endpoint,
                    "Auth rate limit exceeded"
                    );
                    Err(StatusCode::TOO_MANY_REQUESTS)
                }
            }
        })
    }
}
