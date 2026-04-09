use chrono::Utc;
use serde::{Deserialize, Serialize};

/// Favourite - tracks which products users have marked as favourites
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Favourite {
    pub favourite_id: String,
    pub user_id: String,
    pub product_id: String,
    pub created_at: String,
}

impl Favourite {
    pub fn new(user_id: String, product_id: String) -> Self {
        Self {
            favourite_id: uuid::Uuid::new_v4().to_string(),
            user_id,
            product_id,
            created_at: Utc::now().to_rfc3339(),
        }
    }
}

/// Favourite with product details - for API responses
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FavouriteWithProduct {
    pub favourite_id: String,
    pub user_id: String,
    pub product_id: String,
    pub created_at: String,
    pub product_name: String,
    pub product_price: i64,
    pub product_image: String,
    pub product_stock: i32,
    pub product_on_sale: i32,
    pub product_sale_price: Option<i64>,
}
