use sqlx::AnyPool;

use crate::models::PostTag;

#[derive(Debug, sqlx::FromRow)]
pub struct TagCount {
    pub tag: String,
    pub count: i64,
}

#[derive(Clone)]
pub struct PostTagRepository {
    pool: AnyPool,
}

impl PostTagRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    /// Replace all tags on a post with the given set.
    pub async fn set_tags(&self, post_id: &str, tags: &[String]) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM post_tags WHERE post_id = $1"#)
            .bind(post_id)
            .execute(&self.pool)
            .await?;

        for tag in tags {
            let normalized = tag.trim().to_lowercase();
            if normalized.is_empty() {
                continue;
            }
            sqlx::query(r#"INSERT OR IGNORE INTO post_tags (post_id, tag) VALUES ($1, $2)"#)
                .bind(post_id)
                .bind(&normalized)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    /// Get all tags for a single post.
    pub async fn get_tags(&self, post_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query_as::<_, PostTag>(
            r#"SELECT post_id, tag FROM post_tags WHERE post_id = $1 ORDER BY tag"#,
        )
        .bind(post_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.tag).collect())
    }

    /// Batch-fetch tags for multiple posts. Returns a map of post_id -> Vec<tag>.
    pub async fn get_tags_for_posts(
        &self,
        post_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<String>>, sqlx::Error> {
        let mut map: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        if post_ids.is_empty() {
            return Ok(map);
        }

        // Build placeholders: $1, $2, ...
        let placeholders: Vec<String> = (1..=post_ids.len()).map(|i| format!("${}", i)).collect();
        let sql = format!(
            "SELECT post_id, tag FROM post_tags WHERE post_id IN ({}) ORDER BY tag",
            placeholders.join(", ")
        );

        let mut query = sqlx::query_as::<_, PostTag>(&sql);
        for id in post_ids {
            query = query.bind(id);
        }

        let rows = query.fetch_all(&self.pool).await?;
        for row in rows {
            map.entry(row.post_id).or_default().push(row.tag);
        }
        Ok(map)
    }

    /// List all distinct tags with their usage count, ordered by count descending.
    pub async fn list_all_tags(&self) -> Result<Vec<TagCount>, sqlx::Error> {
        sqlx::query_as::<_, TagCount>(
 r#"SELECT tag, COUNT(*) as count FROM post_tags GROUP BY tag ORDER BY count DESC, tag ASC"#,
 )
 .fetch_all(&self.pool)
 .await
    }

    /// Find post_ids that have ALL of the given tags.
    pub async fn search_by_tags(&self, tags: &[String]) -> Result<Vec<String>, sqlx::Error> {
        if tags.is_empty() {
            return Ok(vec![]);
        }

        let placeholders: Vec<String> = (1..=tags.len()).map(|i| format!("${}", i)).collect();
        let tag_count = tags.len() as i64;
        let sql = format!(
 "SELECT post_id FROM post_tags WHERE tag IN ({}) GROUP BY post_id HAVING COUNT(DISTINCT tag) = {}",
 placeholders.join(", "),
 tag_count
 );

        let mut query = sqlx::query_scalar::<_, String>(&sql);
        for tag in tags {
            query = query.bind(tag.trim().to_lowercase());
        }

        query.fetch_all(&self.pool).await
    }
}
