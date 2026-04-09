use crate::models::{SitePage, SitePageLink};
use chrono::Utc;
use sqlx::AnyPool;

#[derive(Clone)]
pub struct SitePageRepository {
    pool: AnyPool,
}

impl SitePageRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    /// Get a published page by slug (public endpoint)
    pub async fn get_published_by_slug(&self, slug: &str) -> Result<Option<SitePage>, sqlx::Error> {
        sqlx::query_as::<_, SitePage>(
            r#"
 SELECT * FROM site_pages
 WHERE slug = $1 AND is_published = 1
 "#,
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
    }

    /// Get a page by slug (admin - includes drafts)
    pub async fn get_by_slug(&self, slug: &str) -> Result<Option<SitePage>, sqlx::Error> {
        sqlx::query_as::<_, SitePage>(
            r#"
 SELECT * FROM site_pages
 WHERE slug = $1
 "#,
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
    }

    /// Get a page by ID (admin)
    pub async fn get_by_id(&self, page_id: i32) -> Result<Option<SitePage>, sqlx::Error> {
        sqlx::query_as::<_, SitePage>(
            r#"
 SELECT * FROM site_pages
 WHERE page_id = $1
 "#,
        )
        .bind(page_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// List all pages (admin)
    pub async fn list_all(&self) -> Result<Vec<SitePage>, sqlx::Error> {
        sqlx::query_as::<_, SitePage>(
            r#"
 SELECT * FROM site_pages
 ORDER BY slug ASC
 "#,
        )
        .fetch_all(&self.pool)
        .await
    }

    /// List published pages (public - for footer)
    pub async fn list_published(&self) -> Result<Vec<SitePageLink>, sqlx::Error> {
        sqlx::query_as::<_, SitePageLink>(
            r#"
 SELECT slug, title FROM site_pages
 WHERE is_published = 1
 ORDER BY slug ASC
 "#,
        )
        .fetch_all(&self.pool)
        .await
    }

    /// Update a page
    pub async fn update(
        &self,
        slug: &str,
        title: &str,
        content: &str,
        meta_description: Option<&str>,
        is_published: bool,
        updated_by: &str,
    ) -> Result<SitePage, sqlx::Error> {
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
 UPDATE site_pages
 SET title = $1,
 content = $2,
 meta_description = $3,
 is_published = $4,
 updated_at = $5,
 updated_by = $6
 WHERE slug = $7
 "#,
        )
        .bind(title)
        .bind(content)
        .bind(meta_description)
        .bind(if is_published { 1 } else { 0 })
        .bind(&now)
        .bind(updated_by)
        .bind(slug)
        .execute(&self.pool)
        .await?;

        self.get_by_slug(slug)
            .await?
            .ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// Create a new page
    pub async fn create(
        &self,
        slug: &str,
        title: &str,
        content: &str,
        meta_description: Option<&str>,
        is_published: bool,
        created_by: &str,
    ) -> Result<SitePage, sqlx::Error> {
        let now = Utc::now().to_rfc3339();

        sqlx::query(
 r#"
 INSERT INTO site_pages (slug, title, content, meta_description, is_published, created_at, updated_at, updated_by)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
 "#,
 )
 .bind(slug)
 .bind(title)
 .bind(content)
 .bind(meta_description)
 .bind(if is_published { 1 } else { 0 })
 .bind(&now)
 .bind(&now)
 .bind(created_by)
 .execute(&self.pool)
 .await?;

        self.get_by_slug(slug)
            .await?
            .ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// Delete a page
    pub async fn delete(&self, slug: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
 DELETE FROM site_pages WHERE slug = $1
 "#,
        )
        .bind(slug)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Toggle publish status
    pub async fn toggle_published(
        &self,
        slug: &str,
        updated_by: &str,
    ) -> Result<SitePage, sqlx::Error> {
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
 UPDATE site_pages
 SET is_published = CASE WHEN is_published = 1 THEN 0 ELSE 1 END,
 updated_at = $1,
 updated_by = $2
 WHERE slug = $3
 "#,
        )
        .bind(&now)
        .bind(updated_by)
        .bind(slug)
        .execute(&self.pool)
        .await?;

        self.get_by_slug(slug)
            .await?
            .ok_or_else(|| sqlx::Error::RowNotFound)
    }
}
