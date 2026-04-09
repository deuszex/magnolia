use axum::{
    Extension,
    extract::{ConnectInfo, Request},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::AnyPool;
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use uuid::Uuid;

// Security Audit Configuration
#[derive(Debug, Clone)]
pub struct AuditConfig {
    pub enabled: bool,
    pub sample_rate: f32, // 0.0 to 1.0
    pub batch_size: usize,
    pub batch_timeout: Duration,
    pub redact_sensitive_headers: bool,
    pub track_request_body: bool,
    pub track_response_body: bool,
    pub geo_lookup_enabled: bool,
    pub fingerprint_enabled: bool,
    pub suspicious_behavior_detection: bool,
    pub max_body_size_to_log: usize,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sample_rate: 1.0,
            batch_size: 100,
            batch_timeout: Duration::from_secs(5),
            redact_sensitive_headers: true,
            track_request_body: false,
            track_response_body: false,
            geo_lookup_enabled: false, // Disabled by default - requires external service
            fingerprint_enabled: true,
            suspicious_behavior_detection: true,
            max_body_size_to_log: 10_000, // 10KB
        }
    }
}

// Audit Entry

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    // Basic Info
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub request_id: String,
    pub correlation_id: Option<String>,

    // User Info
    pub user_id: String,
    pub session_id: Option<String>,
    pub api_key_id: Option<String>,

    // Request Info
    pub method: String,
    pub path: String,
    pub query_params: Option<HashMap<String, String>>,
    pub resource: String,
    pub action: String,
    pub api_version: Option<String>,

    // Network Info
    pub ip_address: Option<IpAddr>,
    pub real_ip: Option<IpAddr>,
    pub forwarded_for: Option<String>,
    pub port: u16,
    pub protocol: String,

    // Client Info
    pub user_agent: Option<String>,
    pub client_version: Option<String>,
    pub accept_language: Option<String>,
    pub accept_encoding: Option<String>,
    pub referer: Option<String>,
    pub origin: Option<String>,

    // Fingerprinting
    pub fingerprint: Option<String>,
    pub device_fingerprint: Option<DeviceFingerprint>,
    pub tls_fingerprint: Option<TlsFingerprint>,

    // Performance Metrics
    pub request_size: usize,
    pub response_size: Option<usize>,
    pub response_time_ms: Option<u64>,
    pub response_status: Option<u16>,

    // Geo Info
    pub geo_info: Option<GeoInfo>,

    // Security Signals
    pub security_signals: SecuritySignals,

    // Headers (sanitized)
    pub request_headers: HashMap<String, String>,
    pub response_headers: Option<HashMap<String, String>>,

    // Body (if enabled and small enough)
    pub request_body_sample: Option<String>,
    pub response_body_sample: Option<String>,

    // Error Info
    pub error_message: Option<String>,
    pub error_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceFingerprint {
    pub screen_resolution: Option<String>,
    pub timezone: Option<String>,
    pub platform: Option<String>,
    pub device_memory: Option<u32>,
    pub hardware_concurrency: Option<u32>,
    pub canvas_fingerprint: Option<String>,
    pub webgl_fingerprint: Option<String>,
    pub audio_fingerprint: Option<String>,
    pub font_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsFingerprint {
    pub ja3_hash: Option<String>,
    pub cipher_suites: Vec<String>,
    pub tls_version: Option<String>,
    pub sni: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoInfo {
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub postal_code: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub timezone: Option<String>,
    pub isp: Option<String>,
    pub org: Option<String>,
    pub asn: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecuritySignals {
    pub is_bot: bool,
    pub bot_score: f32,
    pub is_tor: bool,
    pub is_vpn: bool,
    pub is_proxy: bool,
    pub is_datacenter: bool,
    pub is_suspicious: bool,
    pub risk_score: f32,
    pub rate_limit_exceeded: bool,
    pub unusual_activity: bool,
    pub authentication_anomaly: bool,
}

impl AuditEntry {
    pub async fn create_batch(pool: &AnyPool, entries: Vec<AuditEntry>) -> Result<(), sqlx::Error> {
        if entries.is_empty() {
            return Ok(());
        }

        let mut tx: sqlx::Transaction<'static, sqlx::Any> = pool.begin().await?;

        for entry in entries {
            // Convert IP addresses to strings for database compatibility
            let ip_address_str = entry.ip_address.map(|ip| ip.to_string());
            let real_ip_str = entry.real_ip.map(|ip| ip.to_string());

            sqlx::query(
                r#"
 INSERT INTO audit_logs (
 id, timestamp, request_id, correlation_id, user_id, session_id,
 api_key_id, method, path, query_params, resource, action,
 api_version, ip_address, real_ip, forwarded_for, port, protocol,
 user_agent, client_version, accept_language, accept_encoding,
 referer, origin, fingerprint, device_fingerprint, tls_fingerprint,
 request_size, response_size, response_time_ms, response_status,
 geo_info, security_signals, request_headers, response_headers,
 request_body_sample, response_body_sample, error_message, error_type
 ) VALUES (
 $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
 $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26,
 $27, $28, $29, $30, $31, $32, $33, $34, $35, $36, $37, $38, $39
 )
 "#,
            )
            .bind(entry.id.to_string())
            .bind(entry.timestamp.to_rfc3339())
            .bind(&entry.request_id)
            .bind(&entry.correlation_id)
            .bind(&entry.user_id)
            .bind(&entry.session_id)
            .bind(&entry.api_key_id)
            .bind(&entry.method)
            .bind(&entry.path)
            .bind(serde_json::to_string(&entry.query_params).ok())
            .bind(&entry.resource)
            .bind(&entry.action)
            .bind(&entry.api_version)
            .bind(&ip_address_str)
            .bind(&real_ip_str)
            .bind(&entry.forwarded_for)
            .bind(entry.port as i32)
            .bind(&entry.protocol)
            .bind(&entry.user_agent)
            .bind(&entry.client_version)
            .bind(&entry.accept_language)
            .bind(&entry.accept_encoding)
            .bind(&entry.referer)
            .bind(&entry.origin)
            .bind(&entry.fingerprint)
            .bind(serde_json::to_string(&entry.device_fingerprint).ok())
            .bind(serde_json::to_string(&entry.tls_fingerprint).ok())
            .bind(entry.request_size as i64)
            .bind(entry.response_size.map(|s| s as i64))
            .bind(entry.response_time_ms.map(|t| t as i64))
            .bind(entry.response_status.map(|s| s as i32))
            .bind(serde_json::to_string(&entry.geo_info).ok())
            .bind(serde_json::to_string(&entry.security_signals).ok())
            .bind(serde_json::to_string(&entry.request_headers).ok())
            .bind(serde_json::to_string(&entry.response_headers).ok())
            .bind(&entry.request_body_sample)
            .bind(&entry.response_body_sample)
            .bind(&entry.error_message)
            .bind(&entry.error_type)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct AuditService {
    config: Arc<AuditConfig>,
    tx: mpsc::UnboundedSender<AuditEntry>,
}

impl AuditService {
    pub fn new(config: AuditConfig, pool: AnyPool) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let config = Arc::new(config);

        // Spawn background worker
        tokio::spawn(Self::batch_writer(rx, pool, config.clone()));

        tracing::info!("Audit service initialized");
        Self { config, tx }
    }

    async fn batch_writer(
        mut rx: mpsc::UnboundedReceiver<AuditEntry>,
        pool: AnyPool,
        config: Arc<AuditConfig>,
    ) {
        let mut batch = Vec::with_capacity(config.batch_size);
        let mut last_flush = Instant::now();

        loop {
            tokio::select! {
            Some(entry) = rx.recv() => {
            batch.push(entry);

            if batch.len() >= config.batch_size {
            Self::flush_batch(&pool, &mut batch).await;
            last_flush = Instant::now();
            }
            }
            _ = tokio::time::sleep(config.batch_timeout) => {
            if !batch.is_empty() && last_flush.elapsed() >= config.batch_timeout {
            Self::flush_batch(&pool, &mut batch).await;
            last_flush = Instant::now();
            }
            }
            else => break,
            }
        }
    }

    async fn flush_batch(pool: &AnyPool, batch: &mut Vec<AuditEntry>) {
        if batch.is_empty() {
            return;
        }

        let count = batch.len();
        let entries = std::mem::take(batch);
        match AuditEntry::create_batch(pool, entries).await {
            Ok(_) => {
                tracing::debug!(count = count, "Flushed audit entries");
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to flush audit batch");
            }
        }
    }

    pub fn send(&self, entry: AuditEntry) {
        if !self.config.enabled {
            return;
        }

        // Apply sampling
        if self.config.sample_rate < 1.0 {
            let sample = rand::random::<f32>();
            if sample > self.config.sample_rate {
                return;
            }
        }

        if let Err(e) = self.tx.send(entry) {
            tracing::error!(error = %e, "Failed to send audit entry");
        }
    }
}

fn extract_ip_address(
    headers: &HeaderMap,
    addr: &SocketAddr,
) -> (Option<IpAddr>, Option<IpAddr>, Option<String>) {
    let forwarded_for = headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let real_ip = headers
        .get("x-real-ip")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse().ok())
        .or_else(|| {
            forwarded_for
                .as_ref()
                .and_then(|s| s.split(',').next())
                .and_then(|s| s.trim().parse().ok())
        });

    (Some(addr.ip()), real_ip, forwarded_for)
}

fn sanitize_headers(headers: &HeaderMap, redact_sensitive: bool) -> HashMap<String, String> {
    let sensitive_headers = [
        "authorization",
        "cookie",
        "x-api-key",
        "x-auth-token",
        "x-csrf-token",
        "proxy-authorization",
    ];

    headers
        .iter()
        .filter_map(|(name, value)| {
            let name_str = name.as_str();
            let value_str = value.to_str().ok()?;

            if redact_sensitive && sensitive_headers.contains(&name_str.to_lowercase().as_str()) {
                Some((name_str.to_string(), "[REDACTED]".to_string()))
            } else {
                Some((name_str.to_string(), value_str.to_string()))
            }
        })
        .collect()
}

fn extract_query_params(uri: &axum::http::Uri) -> Option<HashMap<String, String>> {
    uri.query().map(|query| {
        query
            .split('&')
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                let key = parts.next()?;
                let value = parts.next().unwrap_or("");
                Some((key.to_string(), value.to_string()))
            })
            .collect()
    })
}

fn generate_fingerprint(headers: &HeaderMap, ip: Option<IpAddr>) -> String {
    let mut hasher = Sha256::new();

    // Add stable headers to fingerprint
    if let Some(ua) = headers.get("user-agent") {
        hasher.update(ua.as_bytes());
    }
    if let Some(lang) = headers.get("accept-language") {
        hasher.update(lang.as_bytes());
    }
    if let Some(encoding) = headers.get("accept-encoding") {
        hasher.update(encoding.as_bytes());
    }
    if let Some(accept) = headers.get("accept") {
        hasher.update(accept.as_bytes());
    }
    if let Some(ip) = ip {
        hasher.update(ip.to_string().as_bytes());
    }

    format!("{:x}", hasher.finalize())
}

fn detect_bot(user_agent: Option<&str>) -> (bool, f32) {
    let bot_patterns = [
        "bot", "crawler", "spider", "scraper", "wget", "curl", "python", "java", "ruby", "perl",
        "php", "go-http",
    ];

    if let Some(ua) = user_agent {
        let ua_lower = ua.to_lowercase();
        for pattern in &bot_patterns {
            if ua_lower.contains(pattern) {
                return (true, 0.9);
            }
        }
    }

    (false, 0.1)
}

fn extract_device_fingerprint(headers: &HeaderMap) -> Option<DeviceFingerprint> {
    // This would typically come from client-side JavaScript
    // For now, we extract what we can from headers

    let platform = headers
        .get("sec-ch-ua-platform")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    Some(DeviceFingerprint {
        screen_resolution: None,
        timezone: None,
        platform,
        device_memory: None,
        hardware_concurrency: None,
        canvas_fingerprint: None,
        webgl_fingerprint: None,
        audio_fingerprint: None,
        font_fingerprint: None,
    })
}

async fn lookup_geo_info(_ip: IpAddr) -> Option<GeoInfo> {
    // TODO
    // Implement actual geo lookup
    None
}

pub async fn audit_middleware(mut request: Request, next: Next) -> Response {
    // Extract AuditService from extensions
    let audit_service = match request.extensions().get::<AuditService>() {
        Some(service) => service.clone(),
        None => {
            // No audit service configured, pass through
            return next.run(request).await;
        }
    };

    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let headers = request.headers().clone();

    let host = match headers.get("host") {
        Some(host) => host.to_str().unwrap_or_default(),
        None => "",
    };
    if method == "GET" && path == "/health" && host.contains("localhost") {
        return next.run(request).await;
    }

    // Try to get ConnectInfo if available
    let addr = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0)
        .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 0)));

    let start_time = Instant::now();
    // Try to get request ID from header, or generate new one
    let request_id = headers
        .get("x-request-id")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    // Extract user information from AuthMiddleware if available
    let user_id = request
        .extensions()
        .get::<super::auth::AuthMiddleware>()
        .map(|auth| auth.user.user_id.clone())
        .unwrap_or_else(|| "anonymous".to_string());

    let session_id = headers
        .get("x-session-id")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    // Extract network info
    let (ip_address, real_ip, forwarded_for) = extract_ip_address(&headers, &addr);

    // Extract request info
    let query_params = extract_query_params(request.uri());
    let protocol = format!("{:?}", request.version());

    // Determine action from method
    let action = match method.as_str() {
        "GET" | "HEAD" => "READ",
        "POST" => "CREATE",
        "PUT" | "PATCH" => "UPDATE",
        "DELETE" => "DELETE",
        "OPTIONS" => "OPTIONS",
        _ => "UNKNOWN",
    }
    .to_string();

    // Extract client info
    let user_agent = headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let client_version = headers
        .get("x-client-version")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let accept_language = headers
        .get("accept-language")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let accept_encoding = headers
        .get("accept-encoding")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let referer = headers
        .get("referer")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let origin = headers
        .get("origin")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let api_version = headers
        .get("x-api-version")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let correlation_id = headers
        .get("x-correlation-id")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    // Generate fingerprint
    let fingerprint = if audit_service.config.fingerprint_enabled {
        Some(generate_fingerprint(&headers, real_ip.or(ip_address)))
    } else {
        None
    };

    // Extract device fingerprint
    let device_fingerprint = if audit_service.config.fingerprint_enabled {
        extract_device_fingerprint(&headers)
    } else {
        None
    };

    // Bot detection
    let (is_bot, bot_score) = detect_bot(user_agent.as_deref());

    // Security signals
    let mut security_signals = SecuritySignals {
        is_bot,
        bot_score,
        ..Default::default()
    };

    // Calculate risk score based on various factors
    let mut risk_score = bot_score;
    if forwarded_for.is_some() {
        risk_score += 0.1;
    }
    if origin.is_none() && method != "GET" {
        risk_score += 0.2;
    }
    security_signals.risk_score = risk_score.min(1.0);
    security_signals.is_suspicious = risk_score > 0.5;

    // Sanitized headers
    let request_headers = sanitize_headers(&headers, audit_service.config.redact_sensitive_headers);

    // Get request size (approximate)
    let request_size = request_headers.values().map(|v| v.len()).sum::<usize>()
        + path.len()
        + query_params
            .as_ref()
            .map(|q| q.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>())
            .unwrap_or(0);

    // Store audit entry ID in extensions for response tracking
    let audit_id = Uuid::new_v4();
    request.extensions_mut().insert(audit_id);

    // Execute the request
    let response = next.run(request).await;
    let response_time_ms = start_time.elapsed().as_millis() as u64;
    let response_status = response.status().as_u16();

    // Geo lookup (async, non-blocking)
    let geo_info = if audit_service.config.geo_lookup_enabled {
        if let Some(ip) = real_ip.or(ip_address) {
            lookup_geo_info(ip).await
        } else {
            None
        }
    } else {
        None
    };

    // Create audit entry
    let audit_entry = AuditEntry {
        id: audit_id,
        timestamp: Utc::now(),
        request_id,
        correlation_id,
        user_id,
        session_id,
        api_key_id: None,
        method,
        path: path.clone(),
        query_params,
        resource: path,
        action,
        api_version,
        ip_address,
        real_ip,
        forwarded_for,
        port: addr.port(),
        protocol,
        user_agent,
        client_version,
        accept_language,
        accept_encoding,
        referer,
        origin,
        fingerprint,
        device_fingerprint,
        tls_fingerprint: None,
        request_size,
        response_size: None, // Could be calculated from response body
        response_time_ms: Some(response_time_ms),
        response_status: Some(response_status),
        geo_info,
        security_signals,
        request_headers,
        response_headers: None,
        request_body_sample: None,
        response_body_sample: None,
        error_message: if response_status >= 400 {
            Some(format!("HTTP {}", response_status))
        } else {
            None
        },
        error_type: if response_status >= 400 {
            Some(
                StatusCode::from_u16(response_status)
                    .map(|s| s.canonical_reason().unwrap_or("Unknown"))
                    .unwrap_or("Unknown")
                    .to_string(),
            )
        } else {
            None
        },
    };
    // Send to audit service
    audit_service.send(audit_entry);

    response
}
