use crate::models::{Media, MediaFilter};
use sqlx::AnyPool;

#[derive(Clone)]
pub struct MediaRepository {
    pool: AnyPool,
}

impl MediaRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    /// Create a new media entry
    pub async fn create(&self, media: &Media) -> Result<Media, sqlx::Error> {
        sqlx::query(
            r#"
 INSERT INTO media (media_id, owner_id, media_type, storage_path,
 thumbnail_path, filename, mime_type, file_size, duration_seconds,
 width, height, file_hash, description, tags, encryption_nonce,
 is_deleted, origin_server, origin_media_id, is_cached, is_fetching,
 created_at, updated_at)
 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22)
 "#,
        )
        .bind(&media.media_id)
        .bind(&media.owner_id)
        .bind(&media.media_type)
        .bind(&media.storage_path)
        .bind(&media.thumbnail_path)
        .bind(&media.filename)
        .bind(&media.mime_type)
        .bind(media.file_size)
        .bind(media.duration_seconds)
        .bind(media.width)
        .bind(media.height)
        .bind(&media.file_hash)
        .bind(&media.description)
        .bind(&media.tags)
        .bind(&media.encryption_nonce)
        .bind(media.is_deleted)
        .bind(&media.origin_server)
        .bind(&media.origin_media_id)
        .bind(media.is_cached)
        .bind(media.is_fetching)
        .bind(&media.created_at)
        .bind(&media.updated_at)
        .execute(&self.pool)
        .await?;

        self.get_by_id(&media.media_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Get media by ID
    pub async fn get_by_id(&self, media_id: &str) -> Result<Option<Media>, sqlx::Error> {
        sqlx::query_as::<_, Media>(r#"SELECT * FROM media WHERE media_id = $1 AND is_deleted = 0"#)
            .bind(media_id)
            .fetch_optional(&self.pool)
            .await
    }

    /// Get media by file hash (for deduplication)
    pub async fn get_by_hash(
        &self,
        owner_id: &str,
        file_hash: &str,
    ) -> Result<Option<Media>, sqlx::Error> {
        sqlx::query_as::<_, Media>(
            r#"SELECT * FROM media WHERE owner_id = $1 AND file_hash = $2 AND is_deleted = 0"#,
        )
        .bind(owner_id)
        .bind(file_hash)
        .fetch_optional(&self.pool)
        .await
    }

    /// List media for a user (gallery)
    pub async fn list_for_user(
        &self,
        owner_id: &str,
        filter: &MediaFilter,
    ) -> Result<Vec<Media>, sqlx::Error> {
        let limit = filter.limit.unwrap_or(50).min(200) as i64;
        let offset = filter.offset.unwrap_or(0).max(0) as i64;

        // Build parameterized query — track bind index as optional conditions are added.
        // $1 is always owner_id; limit/offset are always the last two.
        let mut query = String::from("SELECT * FROM media WHERE owner_id = $1 AND is_deleted = 0");
        let mut idx = 2usize;

        if filter.media_type.is_some() {
            query.push_str(&format!(" AND media_type = ${}", idx));
            idx += 1;
        }
        if filter.mime_type.is_some() {
            query.push_str(&format!(" AND mime_type LIKE ${}", idx));
            idx += 1;
        }
        if filter.min_size.is_some() {
            query.push_str(&format!(" AND file_size >= ${}", idx));
            idx += 1;
        }
        if filter.max_size.is_some() {
            query.push_str(&format!(" AND file_size <= ${}", idx));
            idx += 1;
        }
        if filter.from_date.is_some() {
            query.push_str(&format!(" AND created_at >= ${}", idx));
            idx += 1;
        }
        if filter.to_date.is_some() {
            query.push_str(&format!(" AND created_at <= ${}", idx));
            idx += 1;
        }

        query.push_str(&format!(
            " ORDER BY created_at DESC LIMIT ${} OFFSET ${}",
            idx,
            idx + 1
        ));

        let mut q = sqlx::query_as::<_, Media>(&query).bind(owner_id);

        if let Some(ref v) = filter.media_type {
            q = q.bind(v);
        }
        if let Some(ref v) = filter.mime_type {
            q = q.bind(format!("{}%", v));
        }
        if let Some(v) = filter.min_size {
            q = q.bind(v);
        }
        if let Some(v) = filter.max_size {
            q = q.bind(v);
        }
        if let Some(ref v) = filter.from_date {
            q = q.bind(v);
        }
        if let Some(ref v) = filter.to_date {
            q = q.bind(v);
        }

        q.bind(limit).bind(offset).fetch_all(&self.pool).await
    }

    /// List images for gallery
    pub async fn list_images(
        &self,
        owner_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<Media>, sqlx::Error> {
        sqlx::query_as::<_, Media>(
            r#"
 SELECT * FROM media
 WHERE owner_id = $1 AND media_type = 'image' AND is_deleted = 0
 ORDER BY created_at DESC
 LIMIT $2 OFFSET $3
 "#,
        )
        .bind(owner_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
    }

    /// List videos for gallery
    pub async fn list_videos(
        &self,
        owner_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<Media>, sqlx::Error> {
        sqlx::query_as::<_, Media>(
            r#"
 SELECT * FROM media
 WHERE owner_id = $1 AND media_type = 'video' AND is_deleted = 0
 ORDER BY created_at DESC
 LIMIT $2 OFFSET $3
 "#,
        )
        .bind(owner_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
    }

    /// List files for repository
    pub async fn list_files(
        &self,
        owner_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<Media>, sqlx::Error> {
        sqlx::query_as::<_, Media>(
            r#"
 SELECT * FROM media
 WHERE owner_id = $1 AND media_type = 'file' AND is_deleted = 0
 ORDER BY created_at DESC
 LIMIT $2 OFFSET $3
 "#,
        )
        .bind(owner_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
    }

    /// Count media by type
    pub async fn count_by_type(
        &self,
        owner_id: &str,
        media_type: &str,
    ) -> Result<i64, sqlx::Error> {
        let result: (i64,) = sqlx::query_as(
 r#"SELECT COUNT(*) FROM media WHERE owner_id = $1 AND media_type = $2 AND is_deleted = 0"#,
 )
 .bind(owner_id)
 .bind(media_type)
 .fetch_one(&self.pool)
 .await?;

        Ok(result.0)
    }

    /// Get total storage used by user
    pub async fn get_storage_used(&self, owner_id: &str) -> Result<i64, sqlx::Error> {
        let result: (i64,) = sqlx::query_as(
 r#"SELECT COALESCE(SUM(file_size), 0) FROM media WHERE owner_id = $1 AND is_deleted = 0"#,
 )
 .bind(owner_id)
 .fetch_one(&self.pool)
 .await?;

        Ok(result.0)
    }

    /// Get storage used by type
    pub async fn get_storage_by_type(
        &self,
        owner_id: &str,
        media_type: &str,
    ) -> Result<i64, sqlx::Error> {
        let result: (i64,) = sqlx::query_as(
 r#"SELECT COALESCE(SUM(file_size), 0) FROM media WHERE owner_id = $1 AND media_type = $2 AND is_deleted = 0"#,
 )
 .bind(owner_id)
 .bind(media_type)
 .fetch_one(&self.pool)
 .await?;

        Ok(result.0)
    }

    /// Update media metadata
    pub async fn update_metadata(
        &self,
        media_id: &str,
        description: Option<&str>,
        tags: Option<&str>,
        updated_at: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
 UPDATE media
 SET description = $1, tags = $2, updated_at = $3
 WHERE media_id = $4
 "#,
        )
        .bind(description)
        .bind(tags)
        .bind(updated_at)
        .bind(media_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Soft delete media
    pub async fn soft_delete(&self, media_id: &str, updated_at: &str) -> Result<(), sqlx::Error> {
        sqlx::query(r#"UPDATE media SET is_deleted = 1, updated_at = $1 WHERE media_id = $2"#)
            .bind(updated_at)
            .bind(media_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Hard delete media
    pub async fn delete(&self, media_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM media WHERE media_id = $1"#)
            .bind(media_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Batch soft delete
    pub async fn batch_soft_delete(
        &self,
        media_ids: &[String],
        updated_at: &str,
    ) -> Result<u64, sqlx::Error> {
        let mut deleted = 0u64;
        for media_id in media_ids {
            let result = sqlx::query(
                r#"UPDATE media SET is_deleted = 1, updated_at = $1 WHERE media_id = $2"#,
            )
            .bind(updated_at)
            .bind(media_id)
            .execute(&self.pool)
            .await?;

            deleted += result.rows_affected();
        }

        Ok(deleted)
    }

    // --- Federated media ---

    /// Find an existing stub or cached row by origin server + remote media_id.
    /// Used to deduplicate stubs when the same remote file appears in multiple posts/messages.
    pub async fn find_by_origin(
        &self,
        origin_server: &str,
        origin_media_id: &str,
    ) -> Result<Option<Media>, sqlx::Error> {
        sqlx::query_as::<_, Media>(
            r#"SELECT * FROM media
               WHERE origin_server = $1 AND origin_media_id = $2 AND is_deleted = 0
               LIMIT 1"#,
        )
        .bind(origin_server)
        .bind(origin_media_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Insert a stub row for a federated media item that has not been fetched yet.
    /// Returns the new local media_id.
    pub async fn create_federated_stub(
        &self,
        local_media_id: &str,
        owner_id: &str, // pass "__fed__" for federated content
        media_type: &str,
        filename: &str,
        mime_type: &str,
        file_size: i64,
        width: Option<i32>,
        height: Option<i32>,
        origin_server: &str,
        origin_media_id: &str,
        now: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO media
               (media_id, owner_id, media_type, storage_path, filename, mime_type,
                file_size, width, height, file_hash, is_deleted,
                origin_server, origin_media_id, is_cached, is_fetching,
                created_at, updated_at)
               VALUES ($1,$2,$3,'',$4,$5,$6,$7,$8,'',$9,$10,$11,0,0,$12,$12)"#,
        )
        .bind(local_media_id)
        .bind(owner_id)
        .bind(media_type)
        .bind(filename)
        .bind(mime_type)
        .bind(file_size)
        .bind(width)
        .bind(height)
        .bind(0i32) // is_deleted
        .bind(origin_server)
        .bind(origin_media_id)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Atomically claim a stub for fetching. Returns true if this caller won the race
    /// (is_fetching was 0 and is now set to 1). Returns false if another fetch is already
    /// in progress or the file is already cached.
    pub async fn claim_fetching(&self, media_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE media SET is_fetching = 1
               WHERE media_id = $1 AND is_cached = 0 AND is_fetching = 0 AND is_deleted = 0"#,
        )
        .bind(media_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Mark a stub as fully cached after a successful fetch.
    pub async fn mark_cached(
        &self,
        media_id: &str,
        storage_path: &str,
        thumbnail_path: Option<&str>,
        file_hash: &str,
        encryption_nonce: Option<&str>,
        updated_at: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE media
               SET storage_path = $1, thumbnail_path = $2, file_hash = $3,
                   encryption_nonce = $4, is_cached = 1, is_fetching = 0, updated_at = $5
               WHERE media_id = $6"#,
        )
        .bind(storage_path)
        .bind(thumbnail_path)
        .bind(file_hash)
        .bind(encryption_nonce)
        .bind(updated_at)
        .bind(media_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Release the fetching lock without marking cached (called on fetch failure).
    pub async fn clear_fetching(&self, media_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE media SET is_fetching = 0 WHERE media_id = $1"#,
        )
        .bind(media_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // --- Block checks ---

    /// Returns true if either party has blocked the other (local user_ids only).
    /// Used before serving media so blocked users can't retrieve each other's files.
    pub async fn is_blocked_local(
        &self,
        user_a: &str,
        user_b: &str,
    ) -> Result<bool, sqlx::Error> {
        let count: (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*) FROM user_blocks
               WHERE (user_id = $1 AND blocked_user_id = $2)
                  OR (user_id = $2 AND blocked_user_id = $1)"#,
        )
        .bind(user_a)
        .bind(user_b)
        .fetch_one(&self.pool)
        .await?;
        Ok(count.0 > 0)
    }

    /// Returns true if the local user has externally banned the remote user from the given peer.
    pub async fn is_externally_banned(
        &self,
        local_user_id: &str,
        server_connection_id: &str,
        remote_user_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let count: (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*) FROM user_external_bans
               WHERE local_user_id = $1
                 AND server_connection_id = $2
                 AND remote_user_id = $3"#,
        )
        .bind(local_user_id)
        .bind(server_connection_id)
        .bind(remote_user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count.0 > 0)
    }

    /// Look up the server_connection_id for a peer by its base_url.
    pub async fn find_connection_id_by_address(
        &self,
        origin_server: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let row: Option<(String,)> = sqlx::query_as(
            r#"SELECT id FROM server_connections WHERE address = $1 AND status = 'active' LIMIT 1"#,
        )
        .bind(origin_server)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(id,)| id))
    }
}
