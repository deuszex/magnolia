//! Embedded static assets for single-binary deployment.
//!
//! This module uses rust-embed to compile frontend assets and templates
//! directly into the binary, enabling distribution as a single executable.
//! ETags are pre-computed at startup for efficient cache validation.

use once_cell::sync::Lazy;
use rust_embed::Embed;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// JavaScript files from web_frontend/js/
#[derive(Embed)]
#[folder = "../web_frontend/js"]
pub struct JsAssets;

/// CSS files from web_frontend/css/
#[derive(Embed)]
#[folder = "../web_frontend/css"]
pub struct CssAssets;

/// Static assets (fonts, images) from web_frontend/assets/
#[derive(Embed)]
#[folder = "../web_frontend/assets"]
pub struct StaticAssets;

/// Locale/i18n JSON files from web_frontend/locales/
#[derive(Embed)]
#[folder = "../web_frontend/locales"]
pub struct LocaleAssets;

/// HTML templates from backend/templates/
#[derive(Embed)]
#[folder = "templates"]
pub struct TemplateAssets;

/// Favicon embedded directly as bytes
pub static FAVICON: &[u8] = include_bytes!("../../web_frontend/favicon.ico");

/// Pre-computed ETag for favicon
pub static FAVICON_ETAG: Lazy<String> = Lazy::new(|| {
    let hash = Sha256::digest(FAVICON);
    format!("\"{}\"", &hex::encode(hash)[..16])
});

/// Pre-computed ETags for all JS assets (computed once at startup)
pub static JS_ETAGS: Lazy<HashMap<String, String>> = Lazy::new(|| compute_etags::<JsAssets>());

/// Pre-computed ETags for all CSS assets
pub static CSS_ETAGS: Lazy<HashMap<String, String>> = Lazy::new(|| compute_etags::<CssAssets>());

/// Pre-computed ETags for all static assets (fonts, images)
pub static STATIC_ETAGS: Lazy<HashMap<String, String>> =
    Lazy::new(|| compute_etags::<StaticAssets>());

/// Pre-computed ETags for all locale files
pub static LOCALE_ETAGS: Lazy<HashMap<String, String>> =
    Lazy::new(|| compute_etags::<LocaleAssets>());

/// Compute ETags for all files in an embedded asset collection
fn compute_etags<E: Embed>() -> HashMap<String, String> {
    let mut etags = HashMap::new();
    for path in E::iter() {
        if let Some(content) = E::get(&path) {
            let hash = Sha256::digest(&content.data);
            // Use first 16 chars of hex hash as ETag (64 bits - sufficient for cache validation)
            let etag = format!("\"{}\"", &hex::encode(hash)[..16]);
            etags.insert(path.to_string(), etag);
        }
    }
    etags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_js_assets_exist() {
        // Verify at least main.js exists
        assert!(
            JsAssets::get("main.js").is_some(),
            "main.js should be embedded"
        );
    }

    #[test]
    fn test_css_assets_exist() {
        // Verify core CSS exists
        assert!(
            CssAssets::get("core-reset.css").is_some(),
            "core-reset.css should be embedded"
        );
    }

    #[test]
    fn test_template_assets_exist() {
        // Verify base template exists
        assert!(
            TemplateAssets::get("base.html").is_some(),
            "base.html should be embedded"
        );
    }

    #[test]
    fn test_favicon_exists() {
        assert!(!FAVICON.is_empty(), "favicon.ico should be embedded");
    }
}
