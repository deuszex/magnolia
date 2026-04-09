use crate::models::ThemeSettings;
use chrono::Utc;
use sqlx::AnyPool;

#[derive(Clone)]
pub struct ThemeRepository {
    pool: AnyPool,
}

impl ThemeRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    /// Get the active theme settings
    pub async fn get_active(&self) -> Result<Option<ThemeSettings>, sqlx::Error> {
        sqlx::query_as::<_, ThemeSettings>(
            r#"
 SELECT * FROM theme_settings
 WHERE is_active = 1
 LIMIT 1
 "#,
        )
        .fetch_optional(&self.pool)
        .await
    }

    /// Update theme settings comprehensively
    pub async fn update(
        &self,
        site_style: &str,
        color_background: &str,
        color_main: &str,
        color_accent: &str,
        color_button: &str,
        color_button_hover: &str,
        color_status_ready: &str,
        color_status_pending: &str,
        color_status_removed: &str,
        background_image: Option<&str>,
        banner_top_left_image: Option<&str>,
        banner_top_left_link: Option<&str>,
        banner_bottom_left_image: Option<&str>,
        banner_bottom_left_link: Option<&str>,
        banner_top_right_image: Option<&str>,
        banner_top_right_link: Option<&str>,
        banner_bottom_right_image: Option<&str>,
        banner_bottom_right_link: Option<&str>,
        site_title: &str,
        brand_icon: &str,
        brand_text: &str,
        favicon_data: Option<&str>,
        hero_title: &str,
        hero_subtitle: Option<&str>,
        updated_by: &str,
    ) -> Result<ThemeSettings, sqlx::Error> {
        let now = Utc::now().to_rfc3339();

        // Get the active theme ID or create one
        let active = self.get_active().await?;

        if let Some(theme) = active {
            // Update existing theme
            sqlx::query(
                r#"
 UPDATE theme_settings
 SET site_style = $1,
 color_background = $2,
 color_main = $3,
 color_accent = $4,
 color_button = $5,
 color_button_hover = $6,
 color_status_ready = $7,
 color_status_pending = $8,
 color_status_removed = $9,
 background_image = $10,
 banner_top_left_image = $11,
 banner_top_left_link = $12,
 banner_bottom_left_image = $13,
 banner_bottom_left_link = $14,
 banner_top_right_image = $15,
 banner_top_right_link = $16,
 banner_bottom_right_image = $17,
 banner_bottom_right_link = $18,
 site_title = $19,
 brand_icon = $20,
 brand_text = $21,
 favicon_data = $22,
 hero_title = $23,
 hero_subtitle = $24,
 updated_at = $25,
 updated_by = $26
 WHERE setting_id = $27
 "#,
            )
            .bind(site_style)
            .bind(color_background)
            .bind(color_main)
            .bind(color_accent)
            .bind(color_button)
            .bind(color_button_hover)
            .bind(color_status_ready)
            .bind(color_status_pending)
            .bind(color_status_removed)
            .bind(background_image)
            .bind(banner_top_left_image)
            .bind(banner_top_left_link)
            .bind(banner_bottom_left_image)
            .bind(banner_bottom_left_link)
            .bind(banner_top_right_image)
            .bind(banner_top_right_link)
            .bind(banner_bottom_right_image)
            .bind(banner_bottom_right_link)
            .bind(site_title)
            .bind(brand_icon)
            .bind(brand_text)
            .bind(favicon_data)
            .bind(hero_title)
            .bind(hero_subtitle)
            .bind(&now)
            .bind(updated_by)
            .bind(theme.setting_id)
            .execute(&self.pool)
            .await?;

            self.get_active()
                .await?
                .ok_or_else(|| sqlx::Error::RowNotFound)
        } else {
            // Create new theme (shouldn't happen with migration default)
            sqlx::query(
 r#"
 INSERT INTO theme_settings (
 site_style,
 color_background, color_main, color_accent,
 color_button, color_button_hover,
 color_status_ready, color_status_pending, color_status_removed,
 background_image,
 banner_top_left_image, banner_top_left_link,
 banner_bottom_left_image, banner_bottom_left_link,
 banner_top_right_image, banner_top_right_link,
 banner_bottom_right_image, banner_bottom_right_link,
 site_title, brand_icon, brand_text, favicon_data,
 hero_title, hero_subtitle,
 updated_at, updated_by, is_active
 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, 1)
 "#,
 )
 .bind(site_style)
 .bind(color_background)
 .bind(color_main)
 .bind(color_accent)
 .bind(color_button)
 .bind(color_button_hover)
 .bind(color_status_ready)
 .bind(color_status_pending)
 .bind(color_status_removed)
 .bind(background_image)
 .bind(banner_top_left_image)
 .bind(banner_top_left_link)
 .bind(banner_bottom_left_image)
 .bind(banner_bottom_left_link)
 .bind(banner_top_right_image)
 .bind(banner_top_right_link)
 .bind(banner_bottom_right_image)
 .bind(banner_bottom_right_link)
 .bind(site_title)
 .bind(brand_icon)
 .bind(brand_text)
 .bind(favicon_data)
 .bind(hero_title)
 .bind(hero_subtitle)
 .bind(&now)
 .bind(updated_by)
 .execute(&self.pool)
 .await?;

            self.get_active()
                .await?
                .ok_or_else(|| sqlx::Error::RowNotFound)
        }
    }

    /// Remove background image
    pub async fn remove_background(&self, updated_by: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
 UPDATE theme_settings
 SET background_image = NULL,
 updated_at = $1,
 updated_by = $2
 WHERE is_active = 1
 "#,
        )
        .bind(&now)
        .bind(updated_by)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Remove a specific banner by position
    pub async fn remove_banner(&self, position: &str, updated_by: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();

        let (image_col, link_col) = match position {
            "top_left" => ("banner_top_left_image", "banner_top_left_link"),
            "bottom_left" => ("banner_bottom_left_image", "banner_bottom_left_link"),
            "top_right" => ("banner_top_right_image", "banner_top_right_link"),
            "bottom_right" => ("banner_bottom_right_image", "banner_bottom_right_link"),
            _ => {
                return Err(sqlx::Error::Protocol(format!(
                    "Invalid banner position: {}",
                    position
                )))
            }
        };

        let query = format!(
            r#"
 UPDATE theme_settings
 SET {} = NULL,
 {} = NULL,
 updated_at = $1,
 updated_by = $2
 WHERE is_active = 1
 "#,
            image_col, link_col
        );

        sqlx::query(&query)
            .bind(&now)
            .bind(updated_by)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Remove favicon
    pub async fn remove_favicon(&self, updated_by: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
 UPDATE theme_settings
 SET favicon_data = NULL,
 updated_at = $1,
 updated_by = $2
 WHERE is_active = 1
 "#,
        )
        .bind(&now)
        .bind(updated_by)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
