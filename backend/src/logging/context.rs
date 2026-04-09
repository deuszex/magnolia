use axum::http::Request;
use uuid::Uuid;

/// Request context for security logging
#[derive(Clone, Debug, Default)]
pub struct RequestContext {
    /// Unique request identifier
    pub request_id: String,
    /// Client IP address
    pub source_ip: Option<String>,
    /// Authenticated user ID
    pub user_id: Option<String>,
    /// Authenticated user email
    pub user_email: Option<String>,
    /// Session ID if authenticated
    pub session_id: Option<String>,
    /// Whether user is an admin
    pub is_admin: bool,
}

impl RequestContext {
    /// Create a new request context with a unique ID
    pub fn new() -> Self {
        Self {
            request_id: Uuid::new_v4().to_string(),
            ..Default::default()
        }
    }

    /// Extract client IP from request headers
    pub fn extract_ip<B>(req: &Request<B>) -> Option<String> {
        // Try X-Forwarded-For first (most common for proxies)
        if let Some(forwarded) = req.headers().get("x-forwarded-for") {
            if let Ok(forwarded_str) = forwarded.to_str() {
                // Take the first IP in the chain (original client)
                if let Some(ip) = forwarded_str.split(',').next() {
                    let ip = ip.trim();
                    if !ip.is_empty() {
                        return Some(ip.to_string());
                    }
                }
            }
        }

        // Try X-Real-IP (used by some proxies like nginx)
        if let Some(real_ip) = req.headers().get("x-real-ip") {
            if let Ok(ip_str) = real_ip.to_str() {
                let ip = ip_str.trim();
                if !ip.is_empty() {
                    return Some(ip.to_string());
                }
            }
        }

        // Try CF-Connecting-IP (Cloudflare)
        if let Some(cf_ip) = req.headers().get("cf-connecting-ip") {
            if let Ok(ip_str) = cf_ip.to_str() {
                let ip = ip_str.trim();
                if !ip.is_empty() {
                    return Some(ip.to_string());
                }
            }
        }

        None
    }

    /// Create context from a request
    pub fn from_request<B>(req: &Request<B>) -> Self {
        Self {
            request_id: Uuid::new_v4().to_string(),
            source_ip: Self::extract_ip(req),
            ..Default::default()
        }
    }

    /// Update context with user information
    pub fn with_user(mut self, user_id: String, email: String, is_admin: bool) -> Self {
        self.user_id = Some(user_id);
        self.user_email = Some(email);
        self.is_admin = is_admin;
        self
    }

    /// Update context with session ID
    pub fn with_session(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }
}
