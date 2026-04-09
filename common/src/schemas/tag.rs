use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateTagRequest {
    #[validate(length(min = 1, max = 100))]
    pub tag_name: String,
    pub parent_tag_id: Option<String>,
    #[validate(length(max = 500))]
    pub description: Option<String>,
    pub display_order: Option<i32>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateTagRequest {
    #[validate(length(min = 1, max = 100))]
    pub tag_name: String,
    pub parent_tag_id: Option<String>,
    #[validate(length(max = 500))]
    pub description: Option<String>,
    pub display_order: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct AddProductTagsRequest {
    pub tag_ids: Vec<String>,
}

// Post tag responses

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagInfo {
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagListResponse {
    pub tags: Vec<TagInfo>,
}
