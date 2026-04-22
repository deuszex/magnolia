use chrono::Utc;
use sqlx::AnyPool;
use uuid::Uuid;

use crate::models::Message;

#[derive(Clone)]
pub struct MessageRepository {
    pool: AnyPool,
}

impl MessageRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, message: &Message) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO messages
               (message_id, conversation_id, sender_id, remote_sender_qualified_id,
                proxy_sender_id, encrypted_content, created_at, federated_status, content_nonce)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
        )
        .bind(&message.message_id)
        .bind(&message.conversation_id)
        .bind(&message.sender_id)
        .bind(&message.remote_sender_qualified_id)
        .bind(&message.proxy_sender_id)
        .bind(&message.encrypted_content)
        .bind(&message.created_at)
        .bind(&message.federated_status)
        .bind(&message.content_nonce)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Create delivery tracking rows for all recipients of a message.
    pub async fn create_deliveries(
        &self,
        message_id: &str,
        recipient_ids: &[String],
    ) -> Result<(), sqlx::Error> {
        for recipient_id in recipient_ids {
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                r#"INSERT INTO message_deliveries (id, message_id, recipient_id, delivered_at)
 VALUES ($1, $2, $3, NULL)"#,
            )
            .bind(&id)
            .bind(message_id)
            .bind(recipient_id)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn get_by_id(&self, message_id: &str) -> Result<Option<Message>, sqlx::Error> {
        sqlx::query_as::<_, Message>(
            r#"SELECT message_id, conversation_id, sender_id, remote_sender_qualified_id,
                      proxy_sender_id, encrypted_content, created_at, federated_status, content_nonce
               FROM messages WHERE message_id = $1"#,
        )
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn list_for_conversation(
        &self,
        conversation_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<Message>, sqlx::Error> {
        sqlx::query_as::<_, Message>(
            r#"SELECT message_id, conversation_id, sender_id, remote_sender_qualified_id,
                      proxy_sender_id, encrypted_content, created_at, federated_status, content_nonce
               FROM messages WHERE conversation_id = $1
               ORDER BY created_at ASC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(conversation_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
    }

    /// Mark a message as delivered for a specific recipient.
    pub async fn mark_delivered(
        &self,
        message_id: &str,
        recipient_id: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"UPDATE message_deliveries SET delivered_at = $1
 WHERE message_id = $2 AND recipient_id = $3 AND delivered_at IS NULL"#,
        )
        .bind(&now)
        .bind(message_id)
        .bind(recipient_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark all messages in a conversation as delivered for a recipient.
    pub async fn mark_conversation_delivered(
        &self,
        conversation_id: &str,
        recipient_id: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"UPDATE message_deliveries SET delivered_at = $1
 WHERE recipient_id = $2 AND delivered_at IS NULL
 AND message_id IN (SELECT message_id FROM messages WHERE conversation_id = $3)"#,
        )
        .bind(&now)
        .bind(recipient_id)
        .bind(conversation_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete(&self, message_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM message_deliveries WHERE message_id = $1"#)
            .bind(message_id)
            .execute(&self.pool)
            .await?;

        sqlx::query(r#"DELETE FROM messages WHERE message_id = $1"#)
            .bind(message_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_for_conversation(&self, conversation_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"DELETE FROM message_deliveries WHERE message_id IN
 (SELECT message_id FROM messages WHERE conversation_id = $1)"#,
        )
        .bind(conversation_id)
        .execute(&self.pool)
        .await?;

        sqlx::query(r#"DELETE FROM messages WHERE conversation_id = $1"#)
            .bind(conversation_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // Attachments

    pub async fn create_attachment(
        &self,
        id: &str,
        message_id: &str,
        media_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO message_attachments (id, message_id, media_id) VALUES ($1, $2, $3)"#,
        )
        .bind(id)
        .bind(message_id)
        .bind(media_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_attachments(
        &self,
        message_id: &str,
    ) -> Result<Vec<crate::models::MessageAttachment>, sqlx::Error> {
        sqlx::query_as::<_, crate::models::MessageAttachment>(
            r#"SELECT id, message_id, media_id FROM message_attachments WHERE message_id = $1"#,
        )
        .bind(message_id)
        .fetch_all(&self.pool)
        .await
    }

    /// Get media_ids shared in a conversation, optionally filtered by media type.
    pub async fn get_conversation_media(
        &self,
        conversation_id: &str,
        media_type: Option<&str>,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<String>, sqlx::Error> {
        if let Some(mt) = media_type {
            sqlx::query_scalar::<_, String>(
                r#"SELECT DISTINCT ma.media_id
 FROM message_attachments ma
 JOIN messages m ON ma.message_id = m.message_id
 JOIN media med ON ma.media_id = med.media_id
 WHERE m.conversation_id = $1 AND med.media_type = $2 AND med.is_deleted = 0
 ORDER BY m.created_at DESC
 LIMIT $3 OFFSET $4"#,
            )
            .bind(conversation_id)
            .bind(mt)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_scalar::<_, String>(
                r#"SELECT DISTINCT ma.media_id
 FROM message_attachments ma
 JOIN messages m ON ma.message_id = m.message_id
 JOIN media med ON ma.media_id = med.media_id
 WHERE m.conversation_id = $1 AND med.is_deleted = 0
 ORDER BY m.created_at DESC
 LIMIT $2 OFFSET $3"#,
            )
            .bind(conversation_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        }
    }

    // --- Federated delivery queue ---

    /// Enqueue an outbound federated message for a specific peer server.
    /// Also sets messages.federated_status = 'pending'.
    #[allow(clippy::too_many_arguments)]
    pub async fn enqueue_federated(
        &self,
        queue_id: &str,
        message_id: &str,
        target_server_id: &str,
        recipient_user_id: &str,
        sender_qualified_id: &str,
        conversation_id: &str,
        conversation_type: &str,
        group_name: Option<&str>,
        encrypted_content: &str,
        sent_at: &str,
        attachments_json: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"INSERT INTO cross_server_message_queue
               (id, message_id, target_server_id, recipient_user_id, sender_qualified_id,
                conversation_id, conversation_type, group_name, encrypted_content,
                sent_at, attachments_json, delivery_status, attempts, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'pending', 0, $12)"#,
        )
        .bind(queue_id)
        .bind(message_id)
        .bind(target_server_id)
        .bind(recipient_user_id)
        .bind(sender_qualified_id)
        .bind(conversation_id)
        .bind(conversation_type)
        .bind(group_name)
        .bind(encrypted_content)
        .bind(sent_at)
        .bind(attachments_json)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"UPDATE messages SET federated_status = 'pending'
               WHERE message_id = $1 AND federated_status IS NULL"#,
        )
        .bind(message_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Mark a queued message as delivered and update messages.federated_status.
    /// If a message had multiple peers and all are now delivered, status becomes 'delivered'.
    pub async fn mark_queue_delivered(&self, queue_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        // Mark this queue entry delivered.
        sqlx::query(
            r#"UPDATE cross_server_message_queue
               SET delivery_status = 'delivered', delivered_at = $1
               WHERE id = $2"#,
        )
        .bind(&now)
        .bind(queue_id)
        .execute(&self.pool)
        .await?;

        // If every queue entry for this message is now delivered, update the message itself.
        sqlx::query(
            r#"UPDATE messages SET federated_status = 'delivered'
               WHERE message_id = (SELECT message_id FROM cross_server_message_queue WHERE id = $1)
                 AND NOT EXISTS (
                     SELECT 1 FROM cross_server_message_queue
                     WHERE message_id = (SELECT message_id FROM cross_server_message_queue WHERE id = $1)
                       AND delivery_status != 'delivered'
                 )"#,
        )
        .bind(queue_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Row type for retrying pending queue entries.
    pub async fn list_pending_for_server(
        &self,
        target_server_id: &str,
    ) -> Result<Vec<FederatedQueueEntry>, sqlx::Error> {
        sqlx::query_as::<_, FederatedQueueEntry>(
            r#"SELECT id, message_id, target_server_id, recipient_user_id,
                      sender_qualified_id, conversation_id, conversation_type,
                      group_name, encrypted_content, sent_at, attachments_json, attempts
               FROM cross_server_message_queue
               WHERE target_server_id = $1 AND delivery_status = 'pending'
               ORDER BY created_at ASC"#,
        )
        .bind(target_server_id)
        .fetch_all(&self.pool)
        .await
    }

    /// Increment attempt counter and update last_attempt_at.
    pub async fn record_attempt(&self, queue_id: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"UPDATE cross_server_message_queue
               SET attempts = attempts + 1, last_attempt_at = $1
               WHERE id = $2"#,
        )
        .bind(&now)
        .bind(queue_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// Pending outbound federated message entry (used for retry drain on peer reconnect).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FederatedQueueEntry {
    pub id: String,
    pub message_id: String,
    pub target_server_id: String,
    pub recipient_user_id: String,
    pub sender_qualified_id: String,
    pub conversation_id: String,
    pub conversation_type: String,
    pub group_name: Option<String>,
    pub encrypted_content: String,
    pub sent_at: String,
    pub attachments_json: String,
    pub attempts: i32,
}
