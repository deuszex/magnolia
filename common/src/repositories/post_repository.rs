use crate::models::{Post, PostContent, PostWithContent};
use sqlx::AnyPool;

#[derive(Clone)]
pub struct PostRepository {
    pool: AnyPool,
}

impl PostRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    /// Create a new post
    pub async fn create(&self, post: &Post) -> Result<Post, sqlx::Error> {
        sqlx::query(
            r#"
 INSERT INTO posts (post_id, author_id, is_published, created_at, updated_at)
 VALUES ($1, $2, $3, $4, $5)
 "#,
        )
        .bind(&post.post_id)
        .bind(&post.author_id)
        .bind(post.is_published)
        .bind(&post.created_at)
        .bind(&post.updated_at)
        .execute(&self.pool)
        .await?;

        self.get_by_id(&post.post_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Add content to a post
    pub async fn add_content(&self, content: &PostContent) -> Result<PostContent, sqlx::Error> {
        sqlx::query(
            r#"
 INSERT INTO post_contents (content_id, post_id, content_type, display_order, content,
 thumbnail_path, original_filename, mime_type, file_size, created_at, content_nonce)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
 "#,
        )
        .bind(&content.content_id)
        .bind(&content.post_id)
        .bind(&content.content_type)
        .bind(content.display_order)
        .bind(&content.content)
        .bind(&content.thumbnail_path)
        .bind(&content.original_filename)
        .bind(&content.mime_type)
        .bind(content.file_size)
        .bind(&content.created_at)
        .bind(&content.content_nonce)
        .execute(&self.pool)
        .await?;

        self.get_content_by_id(&content.content_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Get post by ID
    pub async fn get_by_id(&self, post_id: &str) -> Result<Option<Post>, sqlx::Error> {
        sqlx::query_as::<_, Post>(r#"SELECT * FROM posts WHERE post_id = $1"#)
            .bind(post_id)
            .fetch_optional(&self.pool)
            .await
    }

    /// Get post content by ID
    pub async fn get_content_by_id(
        &self,
        content_id: &str,
    ) -> Result<Option<PostContent>, sqlx::Error> {
        sqlx::query_as::<_, PostContent>(r#"SELECT * FROM post_contents WHERE content_id = $1"#)
            .bind(content_id)
            .fetch_optional(&self.pool)
            .await
    }

    /// Get post with all contents
    pub async fn get_with_contents(
        &self,
        post_id: &str,
    ) -> Result<Option<PostWithContent>, sqlx::Error> {
        let post = self.get_by_id(post_id).await?;

        match post {
            Some(post) => {
                let contents = self.get_contents_for_post(post_id).await?;
                let comment_count = self.get_comment_count(post_id).await?;
                Ok(Some(PostWithContent {
                    post,
                    contents,
                    comment_count,
                }))
            }
            None => Ok(None),
        }
    }

    /// Get all contents for a post
    pub async fn get_contents_for_post(
        &self,
        post_id: &str,
    ) -> Result<Vec<PostContent>, sqlx::Error> {
        sqlx::query_as::<_, PostContent>(
            r#"
 SELECT * FROM post_contents
 WHERE post_id = $1
 ORDER BY display_order ASC
 "#,
        )
        .bind(post_id)
        .fetch_all(&self.pool)
        .await
    }

    /// Get comment count for a post
    pub async fn get_comment_count(&self, post_id: &str) -> Result<i64, sqlx::Error> {
        let result: (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*) FROM comments WHERE post_id = $1 AND is_deleted = 0"#,
        )
        .bind(post_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(result.0)
    }

    /// List published posts (feed)
    pub async fn list_published(&self, limit: i32, offset: i32) -> Result<Vec<Post>, sqlx::Error> {
        sqlx::query_as::<_, Post>(
            r#"
 SELECT * FROM posts
 WHERE is_published = 1
 ORDER BY created_at DESC
 LIMIT $1 OFFSET $2
 "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
    }

    /// List posts by author
    pub async fn list_by_author(
        &self,
        author_id: &str,
        include_drafts: bool,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<Post>, sqlx::Error> {
        if include_drafts {
            sqlx::query_as::<_, Post>(
                r#"
 SELECT * FROM posts
 WHERE author_id = $1
 ORDER BY created_at DESC
 LIMIT $2 OFFSET $3
 "#,
            )
            .bind(author_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, Post>(
                r#"
 SELECT * FROM posts
 WHERE author_id = $1 AND is_published = 1
 ORDER BY created_at DESC
 LIMIT $2 OFFSET $3
 "#,
            )
            .bind(author_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        }
    }

    /// Count total published posts
    pub async fn count_published(&self) -> Result<i64, sqlx::Error> {
        let result: (i64,) = sqlx::query_as(r#"SELECT COUNT(*) FROM posts WHERE is_published = 1"#)
            .fetch_one(&self.pool)
            .await?;

        Ok(result.0)
    }

    /// Update post publish status
    pub async fn set_published(
        &self,
        post_id: &str,
        is_published: i32,
        updated_at: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(r#"UPDATE posts SET is_published = $1, updated_at = $2 WHERE post_id = $3"#)
            .bind(is_published)
            .bind(updated_at)
            .bind(post_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Delete all contents for a post
    pub async fn delete_contents(&self, post_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM post_contents WHERE post_id = $1"#)
            .bind(post_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Delete a post and its contents
    pub async fn delete(&self, post_id: &str) -> Result<(), sqlx::Error> {
        // Delete contents first (foreign key)
        self.delete_contents(post_id).await?;

        sqlx::query(r#"DELETE FROM posts WHERE post_id = $1"#)
            .bind(post_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Search posts with multiple filters.
    /// Builds a dynamic query. All filters are optional.
    pub async fn search_posts(&self, params: &PostSearchParams) -> Result<Vec<Post>, sqlx::Error> {
        let (where_clause, bind_values) = params.build_where_clause();
        let sql = format!(
            "SELECT DISTINCT p.post_id, p.author_id, p.is_published, p.created_at, p.updated_at \
 FROM posts p {} \
 WHERE p.is_published = 1 {} \
 ORDER BY p.created_at DESC \
 LIMIT {} OFFSET {}",
            params.build_joins(),
            where_clause,
            params.limit,
            params.offset,
        );

        let mut query = sqlx::query_as::<_, Post>(&sql);
        for val in &bind_values {
            query = query.bind(val);
        }
        query.fetch_all(&self.pool).await
    }

    /// Count search results for pagination.
    pub async fn count_search_results(
        &self,
        params: &PostSearchParams,
    ) -> Result<i64, sqlx::Error> {
        let (where_clause, bind_values) = params.build_where_clause();
        let sql = format!(
            "SELECT COUNT(DISTINCT p.post_id) \
 FROM posts p {} \
 WHERE p.is_published = 1 {}",
            params.build_joins(),
            where_clause,
        );

        let mut query = sqlx::query_scalar::<_, i64>(&sql);
        for val in &bind_values {
            query = query.bind(val);
        }
        query.fetch_one(&self.pool).await
    }
}

/// Parameters for searching posts with multiple filters.
pub struct PostSearchParams {
    pub text_query: Option<String>,
    pub tags: Option<Vec<String>>,
    pub has_images: bool,
    pub has_videos: bool,
    pub has_files: bool,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub author_id: Option<String>,
    pub limit: i32,
    pub offset: i32,
}

impl PostSearchParams {
    fn build_joins(&self) -> String {
        let mut joins = String::new();
        if self
            .text_query
            .as_ref()
            .map_or(false, |q| !q.trim().is_empty())
        {
            joins.push_str(" JOIN post_contents pc_search ON pc_search.post_id = p.post_id AND pc_search.content_type = 'text'");
        }
        joins
    }

    fn build_where_clause(&self) -> (String, Vec<String>) {
        let mut clauses = Vec::new();
        let mut bind_values: Vec<String> = Vec::new();
        let mut bind_idx = 1usize;

        if let Some(ref q) = self.text_query {
            if !q.trim().is_empty() {
                clauses.push(format!("AND pc_search.content LIKE ${}", bind_idx));
                bind_values.push(format!("%{}%", q.trim()));
                bind_idx += 1;
            }
        }

        if let Some(ref tags) = self.tags {
            let non_empty: Vec<String> = tags
                .iter()
                .map(|t| t.trim().to_lowercase())
                .filter(|t| !t.is_empty())
                .collect();
            if !non_empty.is_empty() {
                let placeholders: Vec<String> = non_empty
                    .iter()
                    .map(|_| {
                        let p = format!("${}", bind_idx);
                        bind_idx += 1;
                        p
                    })
                    .collect();
                clauses.push(format!(
                    "AND p.post_id IN (SELECT post_id FROM post_tags WHERE tag IN ({}) \
 GROUP BY post_id HAVING COUNT(DISTINCT tag) = {})",
                    placeholders.join(", "),
                    non_empty.len()
                ));
                for tag in non_empty {
                    bind_values.push(tag);
                }
            }
        }

        // Content type filters
        let mut content_type_filters = Vec::new();
        if self.has_images {
            content_type_filters.push("'image'");
        }
        if self.has_videos {
            content_type_filters.push("'video'");
        }
        if self.has_files {
            content_type_filters.push("'file'");
        }
        if !content_type_filters.is_empty() {
            clauses.push(format!(
 "AND EXISTS (SELECT 1 FROM post_contents WHERE post_id = p.post_id AND content_type IN ({}))",
 content_type_filters.join(", ")
 ));
        }

        if let Some(ref from) = self.from_date {
            clauses.push(format!("AND p.created_at >= ${}", bind_idx));
            bind_values.push(from.clone());
            bind_idx += 1;
        }

        if let Some(ref to) = self.to_date {
            clauses.push(format!("AND p.created_at <= ${}", bind_idx));
            bind_values.push(to.clone());
            bind_idx += 1;
        }

        if let Some(ref author) = self.author_id {
            clauses.push(format!("AND p.author_id = ${}", bind_idx));
            bind_values.push(author.clone());
            let _ = bind_idx;
        }

        (clauses.join(" "), bind_values)
    }
}
