use serde::Deserialize;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateSitePageRequest {
    #[validate(length(
        min = 1,
        max = 200,
        message = "Title is required and must be 1-200 characters"
    ))]
    pub title: String,

    #[validate(length(min = 1, max = 100000, message = "Content is required"))]
    pub content: String,

    #[validate(length(max = 500, message = "Meta description must be at most 500 characters"))]
    pub meta_description: Option<String>,

    pub is_published: bool,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateSitePageRequest {
    #[validate(
 length(min = 1, max = 100, message = "Slug is required and must be 1-100 characters"),
 regex(path = *SLUG_REGEX, message = "Slug must contain only lowercase letters, numbers, hyphens, and forward slashes")
 )]
    pub slug: String,

    #[validate(length(
        min = 1,
        max = 200,
        message = "Title is required and must be 1-200 characters"
    ))]
    pub title: String,

    #[validate(length(min = 1, max = 100000, message = "Content is required"))]
    pub content: String,

    #[validate(length(max = 500, message = "Meta description must be at most 500 characters"))]
    pub meta_description: Option<String>,

    pub is_published: Option<bool>,
}

lazy_static::lazy_static! {
 static ref SLUG_REGEX: regex::Regex = regex::Regex::new(r"^[a-z0-9][a-z0-9\-/]*[a-z0-9]$|^[a-z0-9]$").unwrap();
}
