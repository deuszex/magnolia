use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateThemeRequest {
    // Site style (design aesthetic)
    #[validate(length(min = 1, max = 20))]
    pub site_style: String,

    // Colors - all hex format
    #[validate(length(min = 4, max = 7))]
    pub color_background: String,

    #[validate(length(min = 4, max = 7))]
    pub color_main: String,

    #[validate(length(min = 4, max = 7))]
    pub color_accent: String,

    #[validate(length(min = 4, max = 7))]
    pub color_button: String,

    #[validate(length(min = 4, max = 7))]
    pub color_button_hover: String,

    #[validate(length(min = 4, max = 7))]
    pub color_status_ready: String,

    #[validate(length(min = 4, max = 7))]
    pub color_status_pending: String,

    #[validate(length(min = 4, max = 7))]
    pub color_status_removed: String,

    // Background image (base64)
    pub background_image: Option<String>,

    // Side banners - 4 positions (base64 + links)
    pub banner_top_left_image: Option<String>,
    pub banner_top_left_link: Option<String>,
    pub banner_bottom_left_image: Option<String>,
    pub banner_bottom_left_link: Option<String>,
    pub banner_top_right_image: Option<String>,
    pub banner_top_right_link: Option<String>,
    pub banner_bottom_right_image: Option<String>,
    pub banner_bottom_right_link: Option<String>,

    // Branding
    #[validate(length(min = 1, max = 200))]
    pub site_title: String,

    #[validate(length(min = 1, max = 10))]
    pub brand_icon: String,

    #[validate(length(min = 1, max = 50))]
    pub brand_text: String,

    // Favicon (base64 .ico file)
    pub favicon_data: Option<String>,

    // Hero section
    #[validate(length(min = 1, max = 200))]
    pub hero_title: String,

    #[validate(length(max = 500))]
    pub hero_subtitle: Option<String>,
}
