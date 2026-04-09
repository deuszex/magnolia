//! Static file serving handlers

use axum::{
    extract::Path,
    http::{StatusCode, header},
    response::Response,
};
use rust_embed::Embed;

use crate::embedded::{
    CSS_ETAGS, CssAssets, FAVICON, FAVICON_ETAG, JS_ETAGS, JsAssets, LOCALE_ETAGS, LocaleAssets,
    STATIC_ETAGS, StaticAssets,
};

pub async fn serve_embedded_js(Path(path): Path<String>) -> Response {
    serve_embedded_file::<JsAssets>(&path, "application/javascript", &JS_ETAGS)
}

pub async fn serve_embedded_css(Path(path): Path<String>) -> Response {
    serve_embedded_file::<CssAssets>(&path, "text/css", &CSS_ETAGS)
}

pub async fn serve_embedded_assets(Path(path): Path<String>) -> Response {
    let mime = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .to_string();
    serve_embedded_file::<StaticAssets>(&path, &mime, &STATIC_ETAGS)
}

pub async fn serve_embedded_locale(Path(filename): Path<String>) -> Response {
    serve_embedded_file::<LocaleAssets>(&filename, "application/json", &LOCALE_ETAGS)
}

pub async fn serve_favicon() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/x-icon")
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .header(header::ETAG, FAVICON_ETAG.as_str())
        .body(axum::body::Body::from(FAVICON.to_vec()))
        .unwrap()
}

/// Public: served without auth (needed for the login page)
pub async fn serve_js_api_file() -> Response {
    serve_embedded_file::<JsAssets>("api.js", "application/javascript", &JS_ETAGS)
}

pub async fn serve_js_auth_page() -> Response {
    serve_embedded_file::<JsAssets>("auth.js", "application/javascript", &JS_ETAGS)
}

pub async fn serve_js_main_file() -> Response {
    serve_embedded_file::<JsAssets>("main.js", "application/javascript", &JS_ETAGS)
}

/// Admin-only: served only to authenticated admins
pub async fn serve_js_admin_file() -> Response {
    serve_embedded_file::<JsAssets>("admin.js", "application/javascript", &JS_ETAGS)
}

pub async fn serve_locale_admin_file() -> Response {
    serve_embedded_file::<LocaleAssets>("admin.json", "application/json", &LOCALE_ETAGS)
}

fn serve_embedded_file<E: Embed>(
    path: &str,
    content_type: &str,
    etags: &std::collections::HashMap<String, String>,
) -> Response {
    match E::get(path) {
        Some(content) => {
            let etag = etags.get(path).cloned().unwrap_or_default();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
                .header(header::ETAG, etag)
                .body(axum::body::Body::from(content.data.to_vec()))
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(axum::body::Body::empty())
            .unwrap(),
    }
}
