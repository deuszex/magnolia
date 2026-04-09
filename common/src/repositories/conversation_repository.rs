use chrono::Utc;
use sqlx::AnyPool;

use crate::models::{Conversation, ConversationMember};

/// Lightweight row for conversation listing with aggregated info.
#[derive(Debug, sqlx::FromRow)]
pub struct ConversationSummaryRow {
    pub conversation_id: String,
    pub conversation_type: String,
    pub name: Option<String>,
    pub member_count: i64,
    pub last_message_at: Option<String>,
}

#[derive(Clone)]
pub struct ConversationRepository {
    pool: AnyPool,
}

impl ConversationRepository {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    // Conversations

    pub async fn create(&self, conv: &Conversation) -> Result<(), sqlx::Error> {
        sqlx::query(
 r#"INSERT INTO conversations (conversation_id, conversation_type, name, created_by, created_at, updated_at)
 VALUES ($1, $2, $3, $4, $5, $6)"#,
 )
 .bind(&conv.conversation_id)
 .bind(&conv.conversation_type)
 .bind(&conv.name)
 .bind(&conv.created_by)
 .bind(&conv.created_at)
 .bind(&conv.updated_at)
 .execute(&self.pool)
 .await?;
        Ok(())
    }

    pub async fn get_by_id(
        &self,
        conversation_id: &str,
    ) -> Result<Option<Conversation>, sqlx::Error> {
        sqlx::query_as::<_, Conversation>(
            r#"SELECT conversation_id, conversation_type, name, created_by, created_at, updated_at
 FROM conversations WHERE conversation_id = $1"#,
        )
        .bind(conversation_id)
        .fetch_optional(&self.pool)
        .await
    }

    /// Find an existing direct conversation between two users.
    pub async fn find_direct(
        &self,
        user_a: &str,
        user_b: &str,
    ) -> Result<Option<Conversation>, sqlx::Error> {
        sqlx::query_as::<_, Conversation>(
 r#"SELECT c.conversation_id, c.conversation_type, c.name, c.created_by, c.created_at, c.updated_at
 FROM conversations c
 WHERE c.conversation_type = 'direct'
 AND EXISTS (SELECT 1 FROM conversation_members WHERE conversation_id = c.conversation_id AND user_id = $1)
 AND EXISTS (SELECT 1 FROM conversation_members WHERE conversation_id = c.conversation_id AND user_id = $2)"#,
 )
 .bind(user_a)
 .bind(user_b)
 .fetch_optional(&self.pool)
 .await
    }

    /// List conversations for a user with member count and last message timestamp.
    pub async fn list_for_user(
        &self,
        user_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<ConversationSummaryRow>, sqlx::Error> {
        sqlx::query_as::<_, ConversationSummaryRow>(
 r#"SELECT c.conversation_id, c.conversation_type, c.name,
 (SELECT COUNT(*) FROM conversation_members WHERE conversation_id = c.conversation_id) AS member_count,
 (SELECT MAX(created_at) FROM messages WHERE conversation_id = c.conversation_id) AS last_message_at
 FROM conversations c
 JOIN conversation_members cm ON cm.conversation_id = c.conversation_id
 WHERE cm.user_id = $1
 ORDER BY COALESCE(
 (SELECT MAX(created_at) FROM messages WHERE conversation_id = c.conversation_id),
 c.created_at
 ) DESC
 LIMIT $2 OFFSET $3"#,
 )
 .bind(user_id)
 .bind(limit)
 .bind(offset)
 .fetch_all(&self.pool)
 .await
    }

    pub async fn update_name(&self, conversation_id: &str, name: &str) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"UPDATE conversations SET name = $1, updated_at = $2 WHERE conversation_id = $3"#,
        )
        .bind(name)
        .bind(&now)
        .bind(conversation_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Delete a conversation and all its members, messages, and deliveries.
    pub async fn delete(&self, conversation_id: &str) -> Result<(), sqlx::Error> {
        // Delete deliveries for messages in this conversation
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

        sqlx::query(r#"DELETE FROM conversation_members WHERE conversation_id = $1"#)
            .bind(conversation_id)
            .execute(&self.pool)
            .await?;

        sqlx::query(r#"DELETE FROM conversations WHERE conversation_id = $1"#)
            .bind(conversation_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // Members

    pub async fn add_member(&self, member: &ConversationMember) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO conversation_members (id, conversation_id, user_id, role, joined_at)
 VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(&member.id)
        .bind(&member.conversation_id)
        .bind(&member.user_id)
        .bind(&member.role)
        .bind(&member.joined_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_member(
        &self,
        conversation_id: &str,
        user_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"DELETE FROM conversation_members WHERE conversation_id = $1 AND user_id = $2"#,
        )
        .bind(conversation_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_member(
        &self,
        conversation_id: &str,
        user_id: &str,
    ) -> Result<Option<ConversationMember>, sqlx::Error> {
        sqlx::query_as::<_, ConversationMember>(
            r#"SELECT id, conversation_id, user_id, role, joined_at
 FROM conversation_members WHERE conversation_id = $1 AND user_id = $2"#,
        )
        .bind(conversation_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn list_members(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<ConversationMember>, sqlx::Error> {
        sqlx::query_as::<_, ConversationMember>(
            r#"SELECT id, conversation_id, user_id, role, joined_at
 FROM conversation_members WHERE conversation_id = $1 ORDER BY joined_at"#,
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn is_member(
        &self,
        conversation_id: &str,
        user_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let row = self.get_member(conversation_id, user_id).await?;
        Ok(row.is_some())
    }

    /// Get unread message counts per conversation for a user.
    pub async fn get_unread_counts(
        &self,
        user_id: &str,
    ) -> Result<std::collections::HashMap<String, i64>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct UnreadRow {
            conversation_id: String,
            unread_count: i64,
        }

        let rows = sqlx::query_as::<_, UnreadRow>(
            r#"SELECT m.conversation_id, COUNT(*) as unread_count
 FROM message_deliveries md
 JOIN messages m ON md.message_id = m.message_id
 WHERE md.recipient_id = $1 AND md.delivered_at IS NULL
 GROUP BY m.conversation_id"#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let mut map = std::collections::HashMap::new();
        for row in rows {
            map.insert(row.conversation_id, row.unread_count);
        }
        Ok(map)
    }
}
