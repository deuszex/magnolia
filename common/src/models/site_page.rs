use serde::{Deserialize, Serialize};

/// Site page for CMS content
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SitePage {
    pub page_id: i32,
    pub slug: String,
    pub title: String,
    pub content: String,
    pub meta_description: Option<String>,
    pub is_published: i32,
    pub created_at: String,
    pub updated_at: String,
    pub updated_by: String,
}

/// Public response (hides internal fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SitePageResponse {
    pub slug: String,
    pub title: String,
    pub content: String,
    pub meta_description: Option<String>,
}

/// Admin list view (summary without full content)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SitePageSummary {
    pub page_id: i32,
    pub slug: String,
    pub title: String,
    pub is_published: bool,
    pub updated_at: String,
}

impl From<SitePage> for SitePageResponse {
    fn from(page: SitePage) -> Self {
        Self {
            slug: page.slug,
            title: page.title,
            content: page.content,
            meta_description: page.meta_description,
        }
    }
}

impl From<SitePage> for SitePageSummary {
    fn from(page: SitePage) -> Self {
        Self {
            page_id: page.page_id,
            slug: page.slug,
            title: page.title,
            is_published: page.is_published == 1,
            updated_at: page.updated_at,
        }
    }
}

/// Minimal page info for footer links (public)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SitePageLink {
    pub slug: String,
    pub title: String,
}

impl From<SitePage> for SitePageLink {
    fn from(page: SitePage) -> Self {
        Self {
            slug: page.slug,
            title: page.title,
        }
    }
}
