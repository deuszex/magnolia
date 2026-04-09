use serde::{Deserialize, Serialize};

/// Theme settings for comprehensive store customization
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThemeSettings {
    pub setting_id: i32,

    // Site style (design aesthetic)
    pub site_style: String,

    // Colors
    pub color_background: String,
    pub color_main: String,
    pub color_accent: String,
    pub color_button: String,
    pub color_button_hover: String,
    pub color_status_ready: String,
    pub color_status_pending: String,
    pub color_status_removed: String,

    // Background image
    pub background_image: Option<String>,

    // Side banners - 4 positions
    pub banner_top_left_image: Option<String>,
    pub banner_top_left_link: Option<String>,
    pub banner_bottom_left_image: Option<String>,
    pub banner_bottom_left_link: Option<String>,
    pub banner_top_right_image: Option<String>,
    pub banner_top_right_link: Option<String>,
    pub banner_bottom_right_image: Option<String>,
    pub banner_bottom_right_link: Option<String>,

    // Branding
    pub site_title: String,
    pub brand_icon: String,
    pub brand_text: String,
    pub favicon_data: Option<String>,

    // Hero section
    pub hero_title: String,
    pub hero_subtitle: Option<String>,

    // Metadata
    pub updated_at: String,
    pub updated_by: String,
    pub is_active: i32,
}

/// Response for theme settings (public-facing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeResponse {
    // Site style (design aesthetic)
    pub site_style: String,

    // Colors
    pub color_background: String,
    pub color_main: String,
    pub color_accent: String,
    pub color_button: String,
    pub color_button_hover: String,
    pub color_status_ready: String,
    pub color_status_pending: String,
    pub color_status_removed: String,

    // Background image
    pub background_image: Option<String>,

    // Side banners - 4 positions
    pub banner_top_left_image: Option<String>,
    pub banner_top_left_link: Option<String>,
    pub banner_bottom_left_image: Option<String>,
    pub banner_bottom_left_link: Option<String>,
    pub banner_top_right_image: Option<String>,
    pub banner_top_right_link: Option<String>,
    pub banner_bottom_right_image: Option<String>,
    pub banner_bottom_right_link: Option<String>,

    // Branding
    pub site_title: String,
    pub brand_icon: String,
    pub brand_text: String,
    pub favicon_data: Option<String>,

    // Hero section
    pub hero_title: String,
    pub hero_subtitle: Option<String>,
}

impl From<ThemeSettings> for ThemeResponse {
    fn from(settings: ThemeSettings) -> Self {
        Self {
            site_style: settings.site_style,
            color_background: settings.color_background,
            color_main: settings.color_main,
            color_accent: settings.color_accent,
            color_button: settings.color_button,
            color_button_hover: settings.color_button_hover,
            color_status_ready: settings.color_status_ready,
            color_status_pending: settings.color_status_pending,
            color_status_removed: settings.color_status_removed,
            background_image: settings.background_image,
            banner_top_left_image: settings.banner_top_left_image,
            banner_top_left_link: settings.banner_top_left_link,
            banner_bottom_left_image: settings.banner_bottom_left_image,
            banner_bottom_left_link: settings.banner_bottom_left_link,
            banner_top_right_image: settings.banner_top_right_image,
            banner_top_right_link: settings.banner_top_right_link,
            banner_bottom_right_image: settings.banner_bottom_right_image,
            banner_bottom_right_link: settings.banner_bottom_right_link,
            site_title: settings.site_title,
            brand_icon: settings.brand_icon,
            brand_text: settings.brand_text,
            favicon_data: settings.favicon_data,
            hero_title: settings.hero_title,
            hero_subtitle: settings.hero_subtitle,
        }
    }
}
