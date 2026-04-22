//! HTTP request handlers

use axum::{
    http::{StatusCode, header},
    response::Response,
};

use crate::embedded::TemplateAssets;

pub mod admin;
pub mod auth;
pub mod calling;
pub mod comment;
pub mod email_html;
pub mod events;
pub mod global_call;
pub mod link_preview;
pub mod media;
pub mod messaging;
pub mod post;
pub mod proxy_user;
pub mod setup;
pub mod static_files;
pub mod tag;
pub mod theme;
pub mod ws;

// Re-export handlers for convenience
pub use auth::*;
pub use static_files::*;

/// Serve main application HTML
pub async fn serve_app() -> Response {
    match TemplateAssets::get("base.html") {
        Some(content) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(axum::body::Body::from(content.data.to_vec()))
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(axum::body::Body::from("Template not found"))
            .unwrap(),
    }
}
