use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::Utc;
use sqlx::AnyPool;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

use crate::config::Settings;
use crate::federation::repo as fed_repo;
use crate::middleware::auth::AuthMiddleware;
use magnolia_common::errors::AppError;
use magnolia_common::models::{Conversation, ConversationMember, Message, UserBlock};
use magnolia_common::repositories::MediaRepository;
use magnolia_common::repositories::{
    ConversationBackgroundRepository, ConversationFavouriteRepository, ConversationRepository,
    MessageRepository, MessagingRepository, UserRepository,
};
use magnolia_common::schemas::media::MediaItemResponse;
use magnolia_common::schemas::{
    AddMemberRequest, BlockListResponse, BlockUserRequest, BlockedUserResponse,
    ChatMessageListResponse, ChatMessageResponse, ConversationBackgroundResponse,
    ConversationListQuery, ConversationListResponse, ConversationMediaQuery, ConversationResponse,
    ConversationSummary, CreateConversationRequest, FavouriteConversationRequest, MemberInfo,
    MessageAttachmentResponse, MessageListQuery, MessagingPreferencesResponse, SendMessageRequest,
    SetBackgroundRequest, UnreadCountsResponse, UpdateConversationRequest,
    UpdateMessagingPreferencesRequest,
};

type AppState = (AnyPool, Arc<Settings>);

// Preferences

/// GET /api/messaging/preferences
pub async fn get_preferences(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
) -> Result<Json<MessagingPreferencesResponse>, AppError> {
    let repo = MessagingRepository::new(pool);
    let prefs = repo
        .get_preferences(&auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get preferences: {}", e)))?;

    let accept = prefs.map(|p| p.accept_messages == 1).unwrap_or(true);
    Ok(Json(MessagingPreferencesResponse {
        accept_messages: accept,
    }))
}

/// PUT /api/messaging/preferences
pub async fn update_preferences(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Json(payload): Json<UpdateMessagingPreferencesRequest>,
) -> Result<Json<MessagingPreferencesResponse>, AppError> {
    payload.validate().map_err(AppError::from)?;

    let repo = MessagingRepository::new(pool);
    let val = if payload.accept_messages { 1 } else { 0 };
    repo.upsert_preferences(&auth.user.user_id, val)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to update preferences: {}", e)))?;

    Ok(Json(MessagingPreferencesResponse {
        accept_messages: payload.accept_messages,
    }))
}

// Blacklist

/// GET /api/messaging/blacklist
pub async fn list_blocks(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
) -> Result<Json<BlockListResponse>, AppError> {
    let repo = MessagingRepository::new(pool);
    let blocks = repo
        .list_blocks(&auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list blocks: {}", e)))?;

    let items = blocks
        .into_iter()
        .map(|b| BlockedUserResponse {
            user_id: b.user_id,
            blocked_user_id: b.blocked_user_id,
            created_at: b.created_at,
        })
        .collect();

    Ok(Json(BlockListResponse { blocks: items }))
}

/// POST /api/messaging/blacklist
pub async fn create_block(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Json(payload): Json<BlockUserRequest>,
) -> Result<(StatusCode, Json<BlockedUserResponse>), AppError> {
    payload.validate().map_err(AppError::from)?;

    if payload.user_id == auth.user.user_id {
        return Err(AppError::BadRequest("Cannot block yourself".to_string()));
    }

    // Verify target user exists
    let user_repo = UserRepository::new(pool.clone());
    let target = user_repo
        .find_by_id(&payload.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to find user: {:?}", e)))?;
    if target.is_none() {
        return Err(AppError::NotFound("User not found".to_string()));
    }

    let now = Utc::now().to_rfc3339();
    let block = UserBlock {
        id: Uuid::new_v4().to_string(),
        user_id: auth.user.user_id.clone(),
        blocked_user_id: payload.user_id.clone(),
        created_at: now.clone(),
    };

    let repo = MessagingRepository::new(pool);
    repo.create_block(&block).await?;

    Ok((
        StatusCode::CREATED,
        Json(BlockedUserResponse {
            user_id: block.user_id,
            blocked_user_id: block.blocked_user_id,
            created_at: block.created_at,
        }),
    ))
}

/// DELETE /api/messaging/blacklist/:user_id
pub async fn delete_block(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(blocked_user_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let repo = MessagingRepository::new(pool);
    repo.delete_block(&auth.user.user_id, &blocked_user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to delete block: {}", e)))?;
    Ok(StatusCode::NO_CONTENT)
}

// Conversations

/// POST /api/conversations
pub async fn create_conversation(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Json(payload): Json<CreateConversationRequest>,
) -> Result<(StatusCode, Json<ConversationResponse>), AppError> {
    payload.validate().map_err(AppError::from)?;

    let conv_type = payload.conversation_type.as_str();
    if conv_type != "direct" && conv_type != "group" {
        return Err(AppError::BadRequest(
            "conversation_type must be 'direct' or 'group'".to_string(),
        ));
    }

    // Cannot add yourself as a member (you're added automatically)
    if payload.member_ids.contains(&auth.user.user_id) {
        return Err(AppError::BadRequest(
            "Do not include yourself in member_ids; you are added automatically".to_string(),
        ));
    }

    // Split member IDs into local user IDs and federated "conn_id:remote_user_id" specs.
    let mut local_ids: Vec<&str> = Vec::new();
    let mut fed_specs: Vec<(&str, &str)> = Vec::new(); // (server_connection_id, remote_user_id)
    for raw in &payload.member_ids {
        if let Some(pos) = raw.find(':') {
            fed_specs.push((&raw[..pos], &raw[pos + 1..]));
        } else {
            local_ids.push(raw.as_str());
        }
    }

    if conv_type == "direct" && (local_ids.len() + fed_specs.len()) != 1 {
        return Err(AppError::BadRequest(
            "Direct conversations require exactly 1 member_id".to_string(),
        ));
    }
    if conv_type == "group" && local_ids.is_empty() && fed_specs.is_empty() {
        return Err(AppError::BadRequest(
            "Group conversations require at least 1 member_id".to_string(),
        ));
    }

    let user_repo = UserRepository::new(pool.clone());
    let messaging_repo = MessagingRepository::new(pool.clone());
    let conv_repo = ConversationRepository::new(pool.clone());

    // Validate local members exist and can be messaged.
    for member_id in &local_ids {
        let target = user_repo
            .find_by_id(member_id)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to find user: {:?}", e)))?;
        if target.is_none() {
            return Err(AppError::NotFound(format!("User {} not found", member_id)));
        }
        messaging_repo
            .can_message(&auth.user.user_id, member_id)
            .await?;
    }

    // Validate federated members are known and their connection is active.
    // Collect (server_connection_id, remote_user_id, remote_qualified_id) triples.
    let mut fed_members: Vec<(String, String, String)> = Vec::new();
    for (sc_id, ru_id) in &fed_specs {
        let conn = fed_repo::get_connection_by_id(&pool, sc_id)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to look up connection: {}", e)))?;
        let conn = conn.ok_or_else(|| AppError::NotFound(format!("Server connection {} not found", sc_id)))?;
        if conn.status != "active" {
            return Err(AppError::BadRequest(format!(
                "Server connection {} is not active", sc_id
            )));
        }
        let fed_user = fed_repo::get_federation_user(&pool, sc_id, ru_id)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to look up federated user: {}", e)))?;
        let fed_user = fed_user.ok_or_else(|| AppError::NotFound(format!("Federated user {}:{} not found", sc_id, ru_id)))?;
        let host = conn.address
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/');
        let ident = if fed_user.username.is_empty() { ru_id.to_string() } else { fed_user.username.clone() };
        let qualified_id = format!("{}@{}", ident, host);
        fed_members.push((sc_id.to_string(), ru_id.to_string(), qualified_id));
    }

    // For direct conversations, return any existing one rather than creating a duplicate.
    if conv_type == "direct" {
        if local_ids.len() == 1 {
            if let Some(existing) = conv_repo
                .find_direct(&auth.user.user_id, local_ids[0])
                .await
                .map_err(|e| AppError::Internal(format!("Failed to check existing conversation: {}", e)))?
            {
                let members = conv_repo
                    .list_members(&existing.conversation_id)
                    .await
                    .map_err(|e| AppError::Internal(format!("Failed to list members: {}", e)))?;
                return Ok((StatusCode::OK, Json(build_conversation_response(existing, members))));
            }
        } else if fed_members.len() == 1 {
            let (sc_id, ru_id, _) = &fed_members[0];
            if let Some(existing_id) = fed_repo::find_federated_dm(&pool, &auth.user.user_id, sc_id, ru_id)
                .await
                .map_err(|e| AppError::Internal(format!("Failed to check existing DM: {}", e)))?
            {
                let conv = conv_repo.get_by_id(&existing_id).await
                    .map_err(|e| AppError::Internal(format!("Failed to get conversation: {}", e)))?
                    .ok_or_else(|| AppError::Internal("Conversation not found".to_string()))?;
                let members = conv_repo.list_members(&existing_id).await
                    .map_err(|e| AppError::Internal(format!("Failed to list members: {}", e)))?;
                return Ok((StatusCode::OK, Json(build_conversation_response(conv, members))));
            }
        }
    }

    let now = Utc::now().to_rfc3339();
    let conversation_id = Uuid::new_v4().to_string();

    let conversation = Conversation {
        conversation_id: conversation_id.clone(),
        conversation_type: conv_type.to_string(),
        name: payload.name.clone(),
        created_by: auth.user.user_id.clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    conv_repo
        .create(&conversation)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create conversation: {}", e)))?;

    // Add creator as owner.
    let creator_role = if conv_type == "group" { "owner" } else { "member" };
    let creator_member = ConversationMember {
        id: Uuid::new_v4().to_string(),
        conversation_id: conversation_id.clone(),
        user_id: auth.user.user_id.clone(),
        role: creator_role.to_string(),
        joined_at: now.clone(),
    };
    conv_repo
        .add_member(&creator_member)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to add creator: {}", e)))?;

    // Add local members.
    let mut all_members = vec![creator_member];
    for member_id in &local_ids {
        let member = ConversationMember {
            id: Uuid::new_v4().to_string(),
            conversation_id: conversation_id.clone(),
            user_id: member_id.to_string(),
            role: "member".to_string(),
            joined_at: now.clone(),
        };
        conv_repo
            .add_member(&member)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to add member: {}", e)))?;
        all_members.push(member);
    }

    // Add federated members.
    for (sc_id, ru_id, qualified_id) in &fed_members {
        fed_repo::add_federated_member(&pool, &conversation_id, sc_id, ru_id, qualified_id)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to add federated member: {}", e)))?;
    }

    Ok((
        StatusCode::CREATED,
        Json(build_conversation_response(conversation, all_members)),
    ))
}

/// GET /api/conversations
pub async fn list_conversations(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Query(query): Query<ConversationListQuery>,
) -> Result<Json<ConversationListResponse>, AppError> {
    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    let conv_repo = ConversationRepository::new(pool.clone());
    let fav_repo = ConversationFavouriteRepository::new(pool.clone());

    let rows = conv_repo
        .list_for_user(&auth.user.user_id, limit, offset)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list conversations: {}", e)))?;

    let unread_map = conv_repo
        .get_unread_counts(&auth.user.user_id)
        .await
        .unwrap_or_default();
    let fav_ids = fav_repo
        .list_for_user(&auth.user.user_id)
        .await
        .unwrap_or_default();

    let user_repo = UserRepository::new(pool.clone());

    let mut conversations = Vec::new();
    for r in rows {
        let unread_count = unread_map.get(&r.conversation_id).copied().unwrap_or(0);
        let is_favourite = fav_ids.contains(&r.conversation_id);

        // For DMs without a name, resolve the other party's display name.
        // For federated DMs the other party is a remote user — look up via federated_conversation_members.
        let display_name = if r.conversation_type == "direct" && r.name.is_none() {
            let members = conv_repo
                .list_members(&r.conversation_id)
                .await
                .unwrap_or_default();
            let other = members
                .iter()
                .find(|m| m.user_id != auth.user.user_id && m.user_id != "__fed__");
            if let Some(other_member) = other {
                user_repo
                    .find_by_id(&other_member.user_id)
                    .await
                    .ok()
                    .flatten()
                    .map(|u| u.display_name.unwrap_or(u.username))
            } else {
                // No local other-party — check for a federated member.
                crate::federation::repo::get_federated_dm_display_name(&pool, &r.conversation_id)
                    .await
                    .unwrap_or(None)
            }
        } else {
            None
        };

        conversations.push(ConversationSummary {
            conversation_id: r.conversation_id,
            conversation_type: r.conversation_type,
            name: r.name,
            display_name,
            member_count: r.member_count,
            last_message_at: r.last_message_at,
            unread_count,
            is_favourite,
        });
    }

    Ok(Json(ConversationListResponse { conversations }))
}

/// GET /api/conversations/:id
pub async fn get_conversation(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(id): Path<String>,
) -> Result<Json<ConversationResponse>, AppError> {
    let conv_repo = ConversationRepository::new(pool);

    // Must be a member
    if !conv_repo
        .is_member(&id, &auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check membership: {}", e)))?
    {
        return Err(AppError::Forbidden);
    }

    let conv = conv_repo
        .get_by_id(&id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get conversation: {}", e)))?;
    let conv = conv.ok_or(AppError::NotFound("Conversation not found".to_string()))?;

    let members = conv_repo
        .list_members(&id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list members: {}", e)))?;

    Ok(Json(build_conversation_response(conv, members)))
}

/// PUT /api/conversations/:id
pub async fn update_conversation(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateConversationRequest>,
) -> Result<impl IntoResponse, AppError> {
    payload.validate().map_err(AppError::from)?;

    let conv_repo = ConversationRepository::new(pool);

    let conv = conv_repo
        .get_by_id(&id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get conversation: {}", e)))?;
    let conv = conv.ok_or(AppError::NotFound("Conversation not found".to_string()))?;

    if conv.conversation_type != "group" {
        return Err(AppError::BadRequest(
            "Cannot rename a direct conversation".to_string(),
        ));
    }

    // Must be owner or admin
    let member = conv_repo
        .get_member(&id, &auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check membership: {}", e)))?;
    let member = member.ok_or(AppError::Forbidden)?;
    if member.role != "owner" && member.role != "admin" {
        return Err(AppError::Forbidden);
    }

    if let Some(ref name) = payload.name {
        conv_repo
            .update_name(&id, name)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to update conversation: {}", e)))?;
    }

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/conversations/:id
pub async fn delete_conversation(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let conv_repo = ConversationRepository::new(pool);

    let conv = conv_repo
        .get_by_id(&id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get conversation: {}", e)))?;
    let conv = conv.ok_or(AppError::NotFound("Conversation not found".to_string()))?;

    let member = conv_repo
        .get_member(&id, &auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check membership: {}", e)))?;
    let member = member.ok_or(AppError::Forbidden)?;

    // Groups: only owner can delete. DMs: either party can delete.
    if conv.conversation_type == "group" && member.role != "owner" {
        return Err(AppError::Forbidden);
    }

    conv_repo
        .delete(&id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to delete conversation: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

// Members

/// POST /api/conversations/:id/members
pub async fn add_member(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(id): Path<String>,
    Json(payload): Json<AddMemberRequest>,
) -> Result<(StatusCode, Json<MemberInfo>), AppError> {
    payload.validate().map_err(AppError::from)?;

    let conv_repo = ConversationRepository::new(pool.clone());

    let conv = conv_repo
        .get_by_id(&id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get conversation: {}", e)))?;
    let conv = conv.ok_or(AppError::NotFound("Conversation not found".to_string()))?;

    if conv.conversation_type != "group" {
        return Err(AppError::BadRequest(
            "Cannot add members to a direct conversation".to_string(),
        ));
    }

    // Must be owner or admin to add members
    let caller = conv_repo
        .get_member(&id, &auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check membership: {}", e)))?;
    let caller = caller.ok_or(AppError::Forbidden)?;
    if caller.role != "owner" && caller.role != "admin" {
        return Err(AppError::Forbidden);
    }

    let now = Utc::now().to_rfc3339();

    // Support federated member IDs in "server_connection_id:remote_user_id" format.
    if let Some(pos) = payload.user_id.find(':') {
        let sc_id = &payload.user_id[..pos];
        let ru_id = &payload.user_id[pos + 1..];

        let conn = fed_repo::get_connection_by_id(&pool, sc_id)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to look up connection: {}", e)))?;
        let conn = conn.ok_or_else(|| AppError::NotFound(format!("Server connection {} not found", sc_id)))?;
        if conn.status != "active" {
            return Err(AppError::BadRequest(format!(
                "Server connection {} is not active", sc_id
            )));
        }
        let fed_user = fed_repo::get_federation_user(&pool, sc_id, ru_id)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to look up federated user: {}", e)))?;
        let fed_user = fed_user.ok_or_else(|| AppError::NotFound("Federated user not found".to_string()))?;
        let host = conn.address
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/');
        let ident = if fed_user.username.is_empty() { ru_id.to_string() } else { fed_user.username.clone() };
        let qualified_id = format!("{}@{}", ident, host);

        fed_repo::add_federated_member(&pool, &id, sc_id, ru_id, &qualified_id)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to add federated member: {}", e)))?;

        return Ok((
            StatusCode::CREATED,
            Json(MemberInfo {
                user_id: payload.user_id.clone(),
                role: "member".to_string(),
                joined_at: now,
            }),
        ));
    }

    // Local member path.
    let user_repo = UserRepository::new(pool.clone());
    let target = user_repo
        .find_by_id(&payload.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to find user: {:?}", e)))?;
    if target.is_none() {
        return Err(AppError::NotFound("User not found".to_string()));
    }

    if conv_repo
        .is_member(&id, &payload.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check membership: {}", e)))?
    {
        return Err(AppError::Conflict("User is already a member".to_string()));
    }

    let messaging_repo = MessagingRepository::new(pool);
    messaging_repo
        .can_message(&auth.user.user_id, &payload.user_id)
        .await?;

    let member = ConversationMember {
        id: Uuid::new_v4().to_string(),
        conversation_id: id,
        user_id: payload.user_id.clone(),
        role: "member".to_string(),
        joined_at: now.clone(),
    };

    conv_repo
        .add_member(&member)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to add member: {}", e)))?;

    Ok((
        StatusCode::CREATED,
        Json(MemberInfo {
            user_id: member.user_id,
            role: member.role,
            joined_at: member.joined_at,
        }),
    ))
}

/// DELETE /api/conversations/:id/members/:user_id
pub async fn remove_member(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path((id, target_user_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let conv_repo = ConversationRepository::new(pool);

    let conv = conv_repo
        .get_by_id(&id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get conversation: {}", e)))?;
    conv.ok_or(AppError::NotFound("Conversation not found".to_string()))?;

    let caller = conv_repo
        .get_member(&id, &auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check membership: {}", e)))?;
    let caller = caller.ok_or(AppError::Forbidden)?;

    // Users can remove themselves (leave), or owner/admin can remove others
    if target_user_id != auth.user.user_id {
        if caller.role != "owner" && caller.role != "admin" {
            return Err(AppError::Forbidden);
        }
    }

    conv_repo
        .remove_member(&id, &target_user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to remove member: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

// Messages

/// POST /api/conversations/:id/messages
pub async fn send_message(
    State((pool, settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    axum::Extension(identity): axum::Extension<
        std::sync::Arc<crate::federation::identity::ServerIdentity>,
    >,
    axum::Extension(s2s_client): axum::Extension<crate::federation::client::S2SClient>,
    Path(id): Path<String>,
    Json(payload): Json<SendMessageRequest>,
) -> Result<(StatusCode, Json<ChatMessageResponse>), AppError> {
    payload.validate().map_err(AppError::from)?;

    let conv_repo = ConversationRepository::new(pool.clone());
    let msg_repo = MessageRepository::new(pool.clone());

    // Must be a member
    if !conv_repo
        .is_member(&id, &auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check membership: {}", e)))?
    {
        return Err(AppError::Forbidden);
    }

    // For DMs, re-check messaging preferences (may have changed)
    let conv = conv_repo
        .get_by_id(&id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get conversation: {}", e)))?;
    let conv = conv.ok_or(AppError::NotFound("Conversation not found".to_string()))?;

    let members = conv_repo
        .list_members(&id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list members: {}", e)))?;

    if conv.conversation_type == "direct" {
        let messaging_repo = MessagingRepository::new(pool.clone());
        for m in &members {
            if m.user_id != auth.user.user_id {
                messaging_repo
                    .can_message(&auth.user.user_id, &m.user_id)
                    .await?;
            }
        }
    }

    let now = Utc::now().to_rfc3339();
    let message_id = Uuid::new_v4().to_string();

    let message = Message {
        message_id: message_id.clone(),
        conversation_id: id.clone(),
        sender_id: auth.user.user_id.clone(),
        remote_sender_qualified_id: None,
        encrypted_content: payload.encrypted_content.clone(),
        created_at: now.clone(),
        federated_status: None,
    };

    msg_repo
        .create(&message)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create message: {}", e)))?;

    // Create delivery rows for all other members
    let recipient_ids: Vec<String> = members
        .iter()
        .filter(|m| m.user_id != auth.user.user_id)
        .map(|m| m.user_id.clone())
        .collect();

    if !recipient_ids.is_empty() {
        msg_repo
            .create_deliveries(&message_id, &recipient_ids)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to create deliveries: {}", e)))?;
    }

    // Forward to any federated members of this conversation.
    {
        let pool_c = pool.clone();
        let settings_c = settings.clone();
        let conv_id = id.clone();
        let content = payload.encrypted_content.clone();
        let sender_id = auth.user.user_id.clone();
        let sent_at = now.clone();
        // Build FederatedMediaRef list from the local media_ids so peers can fetch them.
        let fed_media_repo = MediaRepository::new(pool.clone());
        let mut fed_attachments = Vec::new();
        for mid in &payload.media_ids {
            if let Ok(Some(m)) = fed_media_repo.get_by_id(mid).await {
                fed_attachments.push(crate::federation::models::FederatedMediaRef {
                    media_id: m.media_id,
                    media_type: m.media_type,
                    mime_type: m.mime_type,
                    file_size: m.file_size,
                    filename: m.filename,
                    width: m.width,
                    height: m.height,
                });
            }
        }
        let conv_type = conv.conversation_type.clone();
        let conv_name = conv.name.clone();
        tokio::spawn(crate::federation::messaging::forward_message_to_peers(
            pool_c, settings_c, identity, s2s_client, conv_id, conv_type, conv_name,
            message_id.clone(), content, sender_id, sent_at, fed_attachments,
        ));
    }

    // Create media attachments if any
    let media_repo = MediaRepository::new(pool);
    for media_id in &payload.media_ids {
        let att_id = Uuid::new_v4().to_string();
        msg_repo
            .create_attachment(&att_id, &message_id, media_id)
            .await
            .map_err(|e| {
                tracing::warn!("Failed to create attachment for media {}: {}", media_id, e);
                AppError::Internal("Failed to create message attachment".to_string())
            })?;
    }

    // Build attachment responses
    let mut attachments = Vec::new();
    for media_id in &payload.media_ids {
        if let Ok(Some(media)) = media_repo.get_by_id(media_id).await {
            attachments.push(MessageAttachmentResponse {
                media_id: media.media_id.clone(),
                media_type: media.media_type.clone(),
                filename: Some(media.filename),
                file_size: Some(media.file_size),
                url: format!("/api/media/{}/file", media.media_id),
                thumbnail_url: media
                    .thumbnail_path
                    .as_ref()
                    .map(|_| format!("/api/media/{}/thumbnail", media.media_id)),
                mime_type: Some(media.mime_type),
            });
        }
    }

    let sender_avatar_url = auth
        .user
        .avatar_media_id
        .as_ref()
        .map(|mid| format!("/api/media/{}/thumbnail", mid));
    Ok((
        StatusCode::CREATED,
        Json(ChatMessageResponse {
            message_id: message.message_id,
            conversation_id: message.conversation_id,
            sender_id: message.sender_id,
            sender_email: auth.user.email.clone(),
            sender_name: auth.user.display_name.clone(),
            sender_avatar_url,
            remote_sender_qualified_id: None,
            encrypted_content: message.encrypted_content,
            attachments,
            created_at: message.created_at,
            federated_status: message.federated_status,
        }),
    ))
}

/// GET /api/conversations/:id/messages
pub async fn list_messages(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(id): Path<String>,
    Query(query): Query<MessageListQuery>,
) -> Result<Json<ChatMessageListResponse>, AppError> {
    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    let conv_repo = ConversationRepository::new(pool.clone());
    let msg_repo = MessageRepository::new(pool.clone());

    // Must be a member
    if !conv_repo
        .is_member(&id, &auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check membership: {}", e)))?
    {
        return Err(AppError::Forbidden);
    }

    let messages = msg_repo
        .list_for_conversation(&id, limit + 1, offset)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list messages: {}", e)))?;

    let has_more = messages.len() > limit as usize;

    // Build sender info lookup (email, display_name, avatar)
    let user_repo = UserRepository::new(pool.clone());
    let media_repo = MediaRepository::new(pool);
    let mut sender_info: std::collections::HashMap<
        String,
        (Option<String>, Option<String>, Option<String>),
    > = std::collections::HashMap::new();
    for m in &messages {
        if m.sender_id == "__fed__" {
            continue;
        }
        if !sender_info.contains_key(&m.sender_id) {
            if let Ok(Some(user)) = user_repo.find_by_id(&m.sender_id).await {
                let avatar = user
                    .avatar_media_id
                    .as_ref()
                    .map(|mid| format!("/api/media/{}/thumbnail", mid));
                sender_info.insert(m.sender_id.clone(), (user.email, user.display_name, avatar));
            }
        }
    }

    let mut result_messages: Vec<ChatMessageResponse> = Vec::new();
    for m in messages.into_iter().take(limit as usize) {
        // Fetch attachments for this message
        let mut attachments = Vec::new();
        if let Ok(atts) = msg_repo.get_attachments(&m.message_id).await {
            for att in atts {
                if let Ok(Some(media)) = media_repo.get_by_id(&att.media_id).await {
                    attachments.push(MessageAttachmentResponse {
                        media_id: media.media_id.clone(),
                        media_type: media.media_type.clone(),
                        filename: Some(media.filename),
                        file_size: Some(media.file_size),
                        url: format!("/api/media/{}/file", media.media_id),
                        thumbnail_url: media
                            .thumbnail_path
                            .as_ref()
                            .map(|_| format!("/api/media/{}/thumbnail", media.media_id)),
                        mime_type: Some(media.mime_type),
                    });
                }
            }
        }

        let (email, name, avatar) = if m.remote_sender_qualified_id.is_none() {
            sender_info.get(&m.sender_id).cloned().unwrap_or_default()
        } else {
            Default::default()
        };
        result_messages.push(ChatMessageResponse {
            message_id: m.message_id,
            conversation_id: m.conversation_id,
            sender_id: m.sender_id.clone(),
            sender_email: email,
            sender_name: name,
            sender_avatar_url: avatar,
            remote_sender_qualified_id: m.remote_sender_qualified_id,
            encrypted_content: m.encrypted_content,
            attachments,
            created_at: m.created_at,
            federated_status: m.federated_status,
        });
    }
    let messages = result_messages;

    // Mark fetched messages as delivered for the requesting user
    msg_repo
        .mark_conversation_delivered(&id, &auth.user.user_id)
        .await
        .map_err(|e| {
            tracing::warn!("Failed to mark messages as delivered: {}", e);
            AppError::Internal("Failed to update delivery status".to_string())
        })?;

    Ok(Json(ChatMessageListResponse { messages, has_more }))
}

/// DELETE /api/messages/:id
pub async fn delete_message(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let msg_repo = MessageRepository::new(pool);

    let message = msg_repo
        .get_by_id(&id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get message: {}", e)))?;
    let message = message.ok_or(AppError::NotFound("Message not found".to_string()))?;

    // Only sender can delete their own message
    if message.sender_id != auth.user.user_id {
        return Err(AppError::Forbidden);
    }

    msg_repo
        .delete(&id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to delete message: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

// Favourites

/// POST /api/messaging/favourites
pub async fn add_favourite(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Json(payload): Json<FavouriteConversationRequest>,
) -> Result<impl IntoResponse, AppError> {
    payload.validate().map_err(AppError::from)?;

    let conv_repo = ConversationRepository::new(pool.clone());
    if !conv_repo
        .is_member(&payload.conversation_id, &auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check membership: {}", e)))?
    {
        return Err(AppError::Forbidden);
    }

    let fav_repo = ConversationFavouriteRepository::new(pool);
    fav_repo
        .add(&auth.user.user_id, &payload.conversation_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to add favourite: {}", e)))?;

    Ok(StatusCode::CREATED)
}

/// DELETE /api/messaging/favourites/:conversation_id
pub async fn remove_favourite(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(conversation_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let fav_repo = ConversationFavouriteRepository::new(pool);
    fav_repo
        .remove(&auth.user.user_id, &conversation_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to remove favourite: {}", e)))?;
    Ok(StatusCode::NO_CONTENT)
}

// Unread counts

/// GET /api/messaging/unread
pub async fn get_unread_counts(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
) -> Result<Json<UnreadCountsResponse>, AppError> {
    let conv_repo = ConversationRepository::new(pool);
    let counts = conv_repo
        .get_unread_counts(&auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get unread counts: {}", e)))?;
    Ok(Json(UnreadCountsResponse { counts }))
}

// Conversation media

/// GET /api/conversations/:id/media
pub async fn get_conversation_media(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(id): Path<String>,
    Query(query): Query<ConversationMediaQuery>,
) -> Result<Json<Vec<MediaItemResponse>>, AppError> {
    let conv_repo = ConversationRepository::new(pool.clone());
    if !conv_repo
        .is_member(&id, &auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check membership: {}", e)))?
    {
        return Err(AppError::Forbidden);
    }

    let msg_repo = MessageRepository::new(pool.clone());
    let media_repo = MediaRepository::new(pool);

    let media_ids = msg_repo
        .get_conversation_media(&id, query.media_type.as_deref(), query.limit, query.offset)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get conversation media: {}", e)))?;

    let mut items = Vec::new();
    for media_id in media_ids {
        if let Ok(Some(media)) = media_repo.get_by_id(&media_id).await {
            items.push(MediaItemResponse {
                media_id: media.media_id.clone(),
                media_type: media.media_type,
                filename: media.filename,
                mime_type: media.mime_type,
                file_size: media.file_size,
                url: format!("/api/media/{}/file", media.media_id),
                thumbnail_url: media
                    .thumbnail_path
                    .as_ref()
                    .map(|_| format!("/api/media/{}/thumbnail", media.media_id)),
                duration_seconds: media.duration_seconds,
                width: media.width,
                height: media.height,
                description: media.description,
                tags: media
                    .tags
                    .as_deref()
                    .and_then(|t| serde_json::from_str::<Vec<String>>(t).ok())
                    .unwrap_or_default(),
                created_at: media.created_at,
                updated_at: media.updated_at,
            });
        }
    }

    Ok(Json(items))
}

// Conversation backgrounds

/// GET /api/conversations/{id}/background
pub async fn get_background(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(id): Path<String>,
) -> Result<Json<ConversationBackgroundResponse>, AppError> {
    let conv_repo = ConversationRepository::new(pool.clone());
    if !conv_repo
        .is_member(&id, &auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check membership: {}", e)))?
    {
        return Err(AppError::Forbidden);
    }

    let bg_repo = ConversationBackgroundRepository::new(pool);
    match bg_repo
        .get(&auth.user.user_id, &id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get background: {}", e)))?
    {
        Some(media_id) => Ok(Json(ConversationBackgroundResponse { media_id })),
        None => Err(AppError::NotFound("No background set".into())),
    }
}

/// PUT /api/conversations/{id}/background
pub async fn set_background(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(id): Path<String>,
    Json(payload): Json<SetBackgroundRequest>,
) -> Result<impl IntoResponse, AppError> {
    payload.validate().map_err(AppError::from)?;

    let conv_repo = ConversationRepository::new(pool.clone());
    if !conv_repo
        .is_member(&id, &auth.user.user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check membership: {}", e)))?
    {
        return Err(AppError::Forbidden);
    }

    let bg_repo = ConversationBackgroundRepository::new(pool);
    bg_repo
        .set(&auth.user.user_id, &id, &payload.media_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to set background: {}", e)))?;

    Ok(StatusCode::OK)
}

/// DELETE /api/conversations/{id}/background
pub async fn delete_background(
    State((pool, _settings)): State<AppState>,
    axum::Extension(auth): axum::Extension<AuthMiddleware>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let bg_repo = ConversationBackgroundRepository::new(pool);
    bg_repo
        .delete(&auth.user.user_id, &id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to delete background: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

// Helpers

fn build_conversation_response(
    conv: Conversation,
    members: Vec<ConversationMember>,
) -> ConversationResponse {
    let member_infos = members
        .into_iter()
        .map(|m| MemberInfo {
            user_id: m.user_id,
            role: m.role,
            joined_at: m.joined_at,
        })
        .collect();

    ConversationResponse {
        conversation_id: conv.conversation_id,
        conversation_type: conv.conversation_type,
        name: conv.name,
        members: member_infos,
        created_at: conv.created_at,
        updated_at: conv.updated_at,
    }
}
