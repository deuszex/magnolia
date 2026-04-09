use crate::models::Comment;
use sqlx::AnyPool;

#[derive(Clone)]
pub struct CommentRepository {
    pool: AnyPool,
}

impl CommentRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    /// Create a new comment
    pub async fn create(&self, comment: &Comment) -> Result<Comment, sqlx::Error> {
        sqlx::query(
            r#"
 INSERT INTO comments (comment_id, post_id, author_id, parent_comment_id, content_type,
 content, media_path,
 is_deleted, created_at, updated_at)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
 "#,
        )
        .bind(&comment.comment_id)
        .bind(&comment.post_id)
        .bind(&comment.author_id)
        .bind(&comment.parent_comment_id)
        .bind(&comment.content_type)
        .bind(&comment.content)
        .bind(&comment.media_path)
        .bind(comment.is_deleted)
        .bind(&comment.created_at)
        .bind(&comment.updated_at)
        .execute(&self.pool)
        .await?;

        self.get_by_id(&comment.comment_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Get comment by ID
    pub async fn get_by_id(&self, comment_id: &str) -> Result<Option<Comment>, sqlx::Error> {
        sqlx::query_as::<_, Comment>(r#"SELECT * FROM comments WHERE comment_id = $1"#)
            .bind(comment_id)
            .fetch_optional(&self.pool)
            .await
    }

    /// List comments for a post (top-level only)
    pub async fn list_for_post(
        &self,
        post_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<Comment>, sqlx::Error> {
        sqlx::query_as::<_, Comment>(
            r#"
 SELECT * FROM comments
 WHERE post_id = $1 AND parent_comment_id IS NULL AND is_deleted = 0
 ORDER BY created_at DESC
 LIMIT $2 OFFSET $3
 "#,
        )
        .bind(post_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
    }

    /// List replies to a comment
    pub async fn list_replies(
        &self,
        parent_comment_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<Comment>, sqlx::Error> {
        sqlx::query_as::<_, Comment>(
            r#"
 SELECT * FROM comments
 WHERE parent_comment_id = $1 AND is_deleted = 0
 ORDER BY created_at ASC
 LIMIT $2 OFFSET $3
 "#,
        )
        .bind(parent_comment_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
    }

    /// Count comments for a post
    pub async fn count_for_post(&self, post_id: &str) -> Result<i64, sqlx::Error> {
        let result: (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*) FROM comments WHERE post_id = $1 AND is_deleted = 0"#,
        )
        .bind(post_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(result.0)
    }

    /// Count replies to a comment
    pub async fn count_replies(&self, parent_comment_id: &str) -> Result<i64, sqlx::Error> {
        let result: (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*) FROM comments WHERE parent_comment_id = $1 AND is_deleted = 0"#,
        )
        .bind(parent_comment_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(result.0)
    }

    /// Update comment content
    pub async fn update_content(
        &self,
        comment_id: &str,
        content: &str,
        updated_at: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
 UPDATE comments
 SET content = $1, updated_at = $2
 WHERE comment_id = $3
 "#,
        )
        .bind(content)
        .bind(updated_at)
        .bind(comment_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Soft delete a comment
    pub async fn soft_delete(&self, comment_id: &str, updated_at: &str) -> Result<(), sqlx::Error> {
        sqlx::query(r#"UPDATE comments SET is_deleted = 1, updated_at = $1 WHERE comment_id = $2"#)
            .bind(updated_at)
            .bind(comment_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Hard delete a comment (admin only)
    pub async fn delete(&self, comment_id: &str) -> Result<(), sqlx::Error> {
        // First delete all replies
        sqlx::query(r#"DELETE FROM comments WHERE parent_comment_id = $1"#)
            .bind(comment_id)
            .execute(&self.pool)
            .await?;

        // Then delete the comment
        sqlx::query(r#"DELETE FROM comments WHERE comment_id = $1"#)
            .bind(comment_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Delete all comments for a post
    pub async fn delete_for_post(&self, post_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM comments WHERE post_id = $1"#)
            .bind(post_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
