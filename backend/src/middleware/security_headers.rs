use axum::{extract::Request, http::header, middleware::Next, response::Response};

/// Add security headers to all responses
pub async fn add_security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    // Content Security Policy
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        "default-src 'self'; \
 script-src 'self'; \
 style-src 'self' 'unsafe-inline'; \
 img-src 'self' data: blob: https:; \
 media-src 'self' blob:; \
 connect-src 'self'; \
 font-src 'self'; \
 object-src 'none'; \
 frame-ancestors 'none'; \
 base-uri 'self'; \
 form-action 'self'"
            .parse()
            .unwrap(),
    );

    // Permissions-Policy: allow camera and microphone for WebRTC calls
    headers.insert(
        header::HeaderName::from_static("permissions-policy"),
        "camera=(self), microphone=(self), display-capture=(self)"
            .parse()
            .unwrap(),
    );

    // Prevent clickjacking
    headers.insert(header::X_FRAME_OPTIONS, "DENY".parse().unwrap());

    // Prevent MIME sniffing
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());

    // Enable XSS protection (for older browsers)
    headers.insert(header::X_XSS_PROTECTION, "1; mode=block".parse().unwrap());

    // Referrer Policy
    headers.insert(
        header::REFERRER_POLICY,
        "strict-origin-when-cross-origin".parse().unwrap(),
    );

    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        "max-age=31536000; includeSubDomains".parse().unwrap(),
    );

    response
}
