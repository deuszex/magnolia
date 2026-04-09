use crate::models::{Favourite, FavouriteWithProduct};
use sqlx::AnyPool;

pub struct FavouriteRepository {
    pool: AnyPool,
}

impl FavouriteRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    /// Add a product to favourites
    pub async fn add(&self, favourite: &Favourite) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
 INSERT INTO favourites (favourite_id, user_id, product_id, created_at)
 VALUES ($1, $2, $3, $4)
 "#,
        )
        .bind(&favourite.favourite_id)
        .bind(&favourite.user_id)
        .bind(&favourite.product_id)
        .bind(&favourite.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Remove a product from favourites
    pub async fn remove(&self, user_id: &str, product_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
 DELETE FROM favourites
 WHERE user_id = $1 AND product_id = $2
 "#,
        )
        .bind(user_id)
        .bind(product_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Check if a product is favourited by a user
    pub async fn is_favourited(
        &self,
        user_id: &str,
        product_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar::<_, i32>(
            r#"
 SELECT COUNT(*) FROM favourites
 WHERE user_id = $1 AND product_id = $2
 "#,
        )
        .bind(user_id)
        .bind(product_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(result > 0)
    }

    /// Get all favourites for a user with product details
    pub async fn find_by_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<FavouriteWithProduct>, sqlx::Error> {
        let favourites = sqlx::query_as::<_, FavouriteWithProduct>(
            r#"
 SELECT
 f.favourite_id,
 f.user_id,
 f.product_id,
 f.created_at,
 p.product_name,
 p.net_price as product_price,
 COALESCE(pi.thumbnail, '') as product_image,
 pl.stock as product_stock,
 CASE WHEN pl.sale_percentage > 0 THEN 1 ELSE 0 END as product_on_sale,
 CASE
 WHEN pl.sale_percentage > 0
 THEN p.net_price * (100 - pl.sale_percentage) / 100
 ELSE NULL
 END as product_sale_price
 FROM favourites f
 INNER JOIN products p ON f.product_id = p.product_id
 INNER JOIN product_listings pl ON f.product_id = pl.product_id
 LEFT JOIN (
 SELECT product_id, thumbnail
 FROM product_images
 WHERE image_id = (
 SELECT MIN(image_id)
 FROM product_images pi2
 WHERE pi2.product_id = product_images.product_id
 )
 ) pi ON f.product_id = pi.product_id
 WHERE f.user_id = $1
 ORDER BY f.created_at DESC
 "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(favourites)
    }

    /// Get all product IDs favourited by a user (for quick checks)
    pub async fn find_product_ids_by_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let product_ids = sqlx::query_scalar::<_, String>(
            r#"
 SELECT product_id FROM favourites
 WHERE user_id = $1
 "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(product_ids)
    }

    /// Get count of favourites for a user
    pub async fn count_by_user(&self, user_id: &str) -> Result<i32, sqlx::Error> {
        let count = sqlx::query_scalar::<_, i32>(
            r#"
 SELECT COUNT(*) FROM favourites
 WHERE user_id = $1
 "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(count)
    }

    /// Get users who have favourited a specific product (for notifications)
    pub async fn find_users_by_product(
        &self,
        product_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let user_ids = sqlx::query_scalar::<_, String>(
            r#"
 SELECT user_id FROM favourites
 WHERE product_id = $1
 "#,
        )
        .bind(product_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(user_ids)
    }
}
