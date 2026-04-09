use serde::{Deserialize, Serialize};
use validator::Validate;

// Preferences

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateMessagingPreferencesRequest {
    pub accept_messages: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagingPreferencesResponse {
    pub accept_messages: bool,
}

// Blacklist

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct BlockUserRequest {
    #[validate(length(min = 1))]
    pub user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedUserResponse {
    pub user_id: String,
    pub blocked_user_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockListResponse {
    pub blocks: Vec<BlockedUserResponse>,
}

// Conversations

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateConversationRequest {
    /// "direct" or "group"
    #[validate(length(min = 1))]
    pub conversation_type: String,
    /// Group name (optional, only for groups)
    #[validate(length(max = 200))]
    pub name: Option<String>,
    /// User IDs to add. Direct: exactly 1. Group: at least 1.
    pub member_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateConversationRequest {
    #[validate(length(max = 200))]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct AddMemberRequest {
    #[validate(length(min = 1))]
    pub user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberInfo {
    pub user_id: String,
    pub role: String,
    pub joined_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationResponse {
    pub conversation_id: String,
    pub conversation_type: String,
    pub name: Option<String>,
    pub members: Vec<MemberInfo>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub conversation_id: String,
    pub conversation_type: String,
    pub name: Option<String>,
    /// For DMs: the other party's email/display name
    pub display_name: Option<String>,
    pub member_count: i64,
    pub last_message_at: Option<String>,
    pub unread_count: i64,
    pub is_favourite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationListResponse {
    pub conversations: Vec<ConversationSummary>,
}

#[derive(Debug, Deserialize)]
pub struct ConversationListQuery {
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

// Messages

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct SendMessageRequest {
    /// Base64-encoded E2E-encrypted ciphertext
    #[validate(length(min = 1, max = 65536))]
    pub encrypted_content: String,
    /// Optional media attachments (media_ids from gallery)
    #[serde(default)]
    pub media_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAttachmentResponse {
    pub media_id: String,
    pub media_type: String,
    pub filename: Option<String>,
    pub file_size: Option<i64>,
    pub url: String,
    pub thumbnail_url: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageResponse {
    pub message_id: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub sender_email: Option<String>,
    pub sender_name: Option<String>,
    pub sender_avatar_url: Option<String>,
    /// Present for cross-server messages: `"username@server"`.
    pub remote_sender_qualified_id: Option<String>,
    pub encrypted_content: String,
    pub attachments: Vec<MessageAttachmentResponse>,
    pub created_at: String,
    /// Only present for outbound federated messages sent by this user.
    /// "pending" = not yet acknowledged by remote server. "delivered" = remote ACKed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub federated_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageListResponse {
    pub messages: Vec<ChatMessageResponse>,
    pub has_more: bool,
}

#[derive(Debug, Deserialize)]
pub struct MessageListQuery {
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

// Favourites

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct FavouriteConversationRequest {
    #[validate(length(min = 1))]
    pub conversation_id: String,
}

// Unread counts

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnreadCountsResponse {
    pub counts: std::collections::HashMap<String, i64>,
}

// Conversation media

#[derive(Debug, Deserialize)]
pub struct ConversationMediaQuery {
    pub media_type: Option<String>,
    #[serde(default = "default_media_limit")]
    pub limit: i32,
    #[serde(default)]
    pub offset: i32,
}

fn default_media_limit() -> i32 {
    50
}

// Conversation backgrounds

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct SetBackgroundRequest {
    #[validate(length(min = 1))]
    pub media_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationBackgroundResponse {
    pub media_id: String,
}
