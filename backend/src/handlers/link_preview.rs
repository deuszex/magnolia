use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::Utc;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{net::IpAddr, sync::Arc};

use crate::{config::Settings, middleware::auth::AuthMiddleware};

type AppState = (sqlx::AnyPool, Arc<Settings>);

// Regexes (compiled once)

// Matches any <meta ...> tag (single line, up to 500 chars of attributes)
static META_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<meta\s[^>]{0,600}>").unwrap());

// Extracts property="..." or name="..." from a meta tag
static PROPERTY_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)(?:property|name)\s*=\s*["']([^"'<>]{0,100})["']"#).unwrap());

// Extracts content="..." from a meta tag
static CONTENT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)content\s*=\s*["']([^"'<>]{0,1000})["']"#).unwrap());

// Extracts <title>...</title>
static TITLE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)<title[^>]*>([^<]{0,500})</title>").unwrap());

// Request / response types

#[derive(Deserialize)]
pub struct LinkPreviewQuery {
    pub url: String,
}

#[derive(Serialize)]
pub struct LinkPreviewResponse {
    pub url: String,
    pub domain: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
}

// Handler

pub async fn get_link_preview(
    State((pool, _settings)): State<AppState>,
    axum::Extension(_auth): axum::Extension<AuthMiddleware>,
    Query(params): Query<LinkPreviewQuery>,
) -> impl IntoResponse {
    let url = params.url.trim().to_string();

    // Only allow https://
    if !url.starts_with("https://") {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Only https:// URLs are supported" })),
        )
            .into_response();
    }

    // Extract host (everything between "https://" and the first /,?,#,:)
    let host = match extract_host(&url) {
        Some(h) => h.to_string(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid URL" })),
            )
                .into_response();
        }
    };

    let domain = host.clone();

    // SSRF: resolve hostname and reject private / loopback addresses
    let lookup_target = format!("{}:443", host);
    match tokio::net::lookup_host(&lookup_target).await {
        Ok(addrs) => {
            let addrs: Vec<_> = addrs.collect();
            if addrs.is_empty() || addrs.iter().all(|a| is_private_ip(a.ip())) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "URL resolves to a private address" })),
                )
                    .into_response();
            }
        }
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Could not resolve host" })),
            )
                .into_response();
        }
    }

    // Check cache (24-hour TTL)
    if let Ok(Some(row)) = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>, String, String)>(
 "SELECT url, title, description, image_url, domain, fetched_at FROM link_previews WHERE url = ?",
 )
 .bind(&url)
 .fetch_optional(&pool)
 .await
 {
 let (cached_url, title, description, image_url, cached_domain, fetched_at) = row;
 if let Ok(fetched) = chrono::DateTime::parse_from_rfc3339(&fetched_at) {
 let age = Utc::now().signed_duration_since(fetched.with_timezone(&Utc));
 if age.num_hours() < 24 {
 return Json(LinkPreviewResponse {
 url: cached_url,
 domain: cached_domain,
 title,
 description,
 image_url,
 })
 .into_response();
 }
 }
 }

    // Fetch and parse
    let (title, description, image_url) = fetch_preview(&url).await.unwrap_or((None, None, None));

    // Store in cache (upsert)
    let now = Utc::now().to_rfc3339();
    let _ = sqlx::query(
        "INSERT INTO link_previews (url, title, description, image_url, domain, fetched_at)
 VALUES (?, ?, ?, ?, ?, ?)
 ON CONFLICT(url) DO UPDATE SET
 title=excluded.title,
 description=excluded.description,
 image_url=excluded.image_url,
 domain=excluded.domain,
 fetched_at=excluded.fetched_at",
    )
    .bind(&url)
    .bind(&title)
    .bind(&description)
    .bind(&image_url)
    .bind(&domain)
    .bind(&now)
    .execute(&pool)
    .await;

    Json(LinkPreviewResponse {
        url,
        domain,
        title,
        description,
        image_url,
    })
    .into_response()
}

// Helpers

/// Extract the hostname from an https:// URL (no port).
fn extract_host(url: &str) -> Option<&str> {
    let without_scheme = url.strip_prefix("https://")?;
    let end = without_scheme
        .find(|c| c == '/' || c == '?' || c == '#' || c == ':')
        .unwrap_or(without_scheme.len());
    let host = &without_scheme[..end];
    if host.is_empty() { None } else { Some(host) }
}

/// Returns true if the IP is in a private, loopback, or link-local range.
fn is_private_ip(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(ip) => {
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_unspecified()
                || ip.is_documentation()
        }
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unspecified(),
    }
}

/// Returns true if the URL is a YouTube video link.
fn is_youtube_url(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    (u.contains("youtube.com/watch") && u.contains("v="))
        || u.contains("youtu.be/")
        || u.contains("youtube.com/shorts/")
        || u.contains("youtube.com/embed/")
}

#[derive(serde::Deserialize)]
struct OEmbedResponse {
    title: Option<String>,
    author_name: Option<String>,
    thumbnail_url: Option<String>,
}

/// Fetch preview via YouTube's public oEmbed API (no key required).
async fn fetch_youtube_preview(
    url: &str,
) -> Result<(Option<String>, Option<String>, Option<String>), anyhow::Error> {
    let oembed_url = format!(
        "https://www.youtube.com/oembed?url={}&format=json",
        urlencoding::encode(url)
    );

    let client: reqwest::Client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let resp = client.get(&oembed_url).send().await?;

    if !resp.status().is_success() {
        return Ok((None, None, None));
    }

    let oembed: OEmbedResponse = resp.json().await?;
    let title = oembed.title;
    let description = oembed.author_name.map(|a| format!("by {}", a));
    // oEmbed returns hqdefault; swap to maxresdefault for a sharper thumbnail
    let image_url = oembed
        .thumbnail_url
        .map(|t| t.replace("/hqdefault.jpg", "/maxresdefault.jpg"));

    Ok((title, description, image_url))
}

/// Fetch the URL and extract OG metadata. Returns (title, description, image_url).
async fn fetch_preview(
    url: &str,
) -> Result<(Option<String>, Option<String>, Option<String>), anyhow::Error> {
    if is_youtube_url(url) {
        return fetch_youtube_preview(url).await;
    }

    let client = reqwest::Client::builder()
 .timeout(std::time::Duration::from_secs(5))
 .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
 .redirect(reqwest::redirect::Policy::limited(5))
 .build()?;

    let response = client.get(url).send().await?;

    // Only parse HTML responses
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    if !content_type.contains("text/html") {
        return Ok((None, None, None));
    }

    // Read at most 64 KB, enough to cover any <head> section
    let bytes = response.bytes().await?;
    let truncated = &bytes[..bytes.len().min(65536)];
    let html = String::from_utf8_lossy(truncated);

    let title = extract_og_tag(&html, "og:title")
        .or_else(|| extract_title(&html))
        .map(|s| html_decode(s.trim()))
        .filter(|s| !s.is_empty());

    let description = extract_og_tag(&html, "og:description")
        .map(|s| html_decode(s.trim()))
        .filter(|s| !s.is_empty());

    // Only accept https:// images to avoid mixed-content warnings
    let image_url = extract_og_tag(&html, "og:image")
        .map(|s| s.trim().to_string())
        .filter(|s| s.starts_with("https://"));

    Ok((title, description, image_url))
}

/// Scan all <meta> tags for one matching `property` and return its `content`.
fn extract_og_tag<'a>(html: &'a str, property: &str) -> Option<String> {
    for m in META_RE.find_iter(html) {
        let tag = m.as_str();
        if let Some(prop_caps) = PROPERTY_RE.captures(tag) {
            if prop_caps
                .get(1)
                .map(|p| p.as_str().eq_ignore_ascii_case(property))
                .unwrap_or(false)
            {
                if let Some(content_caps) = CONTENT_RE.captures(tag) {
                    return content_caps.get(1).map(|c| c.as_str().to_string());
                }
            }
        }
    }
    None
}

fn extract_title(html: &str) -> Option<String> {
    TITLE_RE
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn html_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}
