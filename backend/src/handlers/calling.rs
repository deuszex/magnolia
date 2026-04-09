use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use chrono::Utc;
use sqlx::AnyPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::config::Settings;
use crate::handlers::ws::{ConnectionRegistry, send_to_user};
use crate::middleware::auth::AuthMiddleware;
use crate::turn;
use magnolia_common::errors::AppError;
use magnolia_common::models::{Call, CallParticipant};
use magnolia_common::repositories::{CallRepository, ConversationRepository, StunServerRepository, UserRepository};
use magnolia_common::schemas::{
    CallHistoryEntry, CallHistoryQuery, CallHistoryResponse, CallParticipantResponse, CallResponse,
    IceConfigResponse, IceServer, InitiateCallRequest,
};

type AppState = (AnyPool, Arc<Settings>);

/// GET /api/calls/ice-config
/// Returns STUN/TURN server configuration for WebRTC peer connections.
/// Servers are read from the database (admin-managed).  Only enabled entries
/// whose last health-check status is not "unreachable" are included.
pub async fn get_ice_config(
    State((pool, _settings)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
) -> Result<Json<IceConfigResponse>, AppError> {
    let stun_repo = StunServerRepository::new(pool.clone());
    let db_servers = stun_repo.list_enabled().await.unwrap_or_default();

    let mut ice_servers: Vec<IceServer> = db_servers
        .into_iter()
        .filter(|s| s.last_status != "unreachable")
        .map(|s| IceServer {
            urls: vec![s.url],
            username: s.username,
            credential: s.credential,
        })
        .collect();

    // Add embedded TURN server if configured
    let turn_config = match turn::TurnConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("TURN config error in ice-config handler: {e}");
            return Ok(Json(IceConfigResponse { ice_servers }));
        }
    };
    if turn_config.enabled {
        let (username, credential) = turn::generate_turn_credentials(
            &auth.user.user_id,
            &turn_config.auth_secret,
            86400,
        );
        ice_servers.push(IceServer {
            urls: vec![
                format!("turn:{}:3478", turn_config.external_ip),
                format!("turn:{}:3478?transport=tcp", turn_config.external_ip),
            ],
            username: Some(username),
            credential: Some(credential),
        });
    }

    Ok(Json(IceConfigResponse { ice_servers }))
}

/// GET /api/calls/history
/// Returns call history for the authenticated user across all conversations.
pub async fn list_call_history(
    State((pool, _settings)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Query(query): Query<CallHistoryQuery>,
) -> Result<Json<CallHistoryResponse>, AppError> {
    let call_repo = CallRepository::new(pool.clone());
    let user_repo = UserRepository::new(pool.clone());
    let conv_repo = ConversationRepository::new(pool);

    let limit = query.limit.min(100);
    let calls = call_repo
        .list_for_user(&auth.user.user_id, limit + 1, query.offset)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list calls: {}", e)))?;

    let has_more = calls.len() > limit as usize;

    let mut entries = Vec::new();
    for call in calls.into_iter().take(limit as usize) {
        let entry = build_call_history_entry(&call, &call_repo, &user_repo, &conv_repo).await;
        entries.push(entry);
    }

    Ok(Json(CallHistoryResponse {
        calls: entries,
        has_more,
    }))
}

/// GET /api/conversations/{id}/calls
/// Returns call history for a specific conversation.
pub async fn list_conversation_calls(
    State((pool, _settings)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Path(id): Path<String>,
    Query(query): Query<CallHistoryQuery>,
) -> Result<Json<CallHistoryResponse>, AppError> {
    let conv_repo = ConversationRepository::new(pool.clone());
    let call_repo = CallRepository::new(pool.clone());
    let user_repo = UserRepository::new(pool);

    // Verify membership
    if !conv_repo
        .is_member(&id, &auth.user.user_id)
        .await
        .unwrap_or(false)
    {
        return Err(AppError::Forbidden);
    }

    let limit = query.limit.min(100);
    let calls = call_repo
        .list_for_conversation(&id, limit + 1, query.offset)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list calls: {}", e)))?;

    let has_more = calls.len() > limit as usize;

    let mut entries = Vec::new();
    for call in calls.into_iter().take(limit as usize) {
        let entry = build_call_history_entry(&call, &call_repo, &user_repo, &conv_repo).await;
        entries.push(entry);
    }

    Ok(Json(CallHistoryResponse {
        calls: entries,
        has_more,
    }))
}

/// GET /api/conversations/{id}/active-call
/// Returns the currently active call in a conversation, if any.
pub async fn get_active_call(
    State((pool, _settings)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Path(id): Path<String>,
) -> Result<Json<Option<CallResponse>>, AppError> {
    let conv_repo = ConversationRepository::new(pool.clone());
    let call_repo = CallRepository::new(pool.clone());
    let user_repo = UserRepository::new(pool);

    // Verify membership
    if !conv_repo
        .is_member(&id, &auth.user.user_id)
        .await
        .unwrap_or(false)
    {
        return Err(AppError::Forbidden);
    }

    let call = call_repo
        .get_active_call(&id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check active call: {}", e)))?;

    match call {
        None => Ok(Json(None)),
        Some(call) => {
            let participants = call_repo
                .list_participants(&call.call_id)
                .await
                .unwrap_or_default();

            let mut participant_responses = Vec::new();
            for p in &participants {
                let display_name = user_repo
                    .find_by_id(&p.user_id)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|u| u.display_name);
                participant_responses.push(CallParticipantResponse {
                    user_id: p.user_id.clone(),
                    display_name,
                    role: p.role.clone(),
                    status: p.status.clone(),
                });
            }

            Ok(Json(Some(CallResponse {
                call_id: call.call_id,
                conversation_id: call.conversation_id,
                call_type: call.call_type,
                status: call.status,
                initiated_by: call.initiated_by,
                participants: participant_responses,
                created_at: call.created_at,
            })))
        }
    }
}

/// POST /api/conversations/{id}/calls
/// REST endpoint to initiate a call (alternative to WebSocket signal).
pub async fn initiate_call(
    State((pool, _settings)): State<AppState>,
    Extension(auth): Extension<AuthMiddleware>,
    Extension(registry): Extension<ConnectionRegistry>,
    Path(id): Path<String>,
    Json(payload): Json<InitiateCallRequest>,
) -> Result<Json<CallResponse>, AppError> {
    let conv_repo = ConversationRepository::new(pool.clone());
    let call_repo = CallRepository::new(pool.clone());
    let user_repo = UserRepository::new(pool.clone());

    // Verify membership
    if !conv_repo
        .is_member(&id, &auth.user.user_id)
        .await
        .unwrap_or(false)
    {
        return Err(AppError::Forbidden);
    }

    // Check for existing active call
    if call_repo
        .get_active_call(&id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to check active call: {}", e)))?
        .is_some()
    {
        return Err(AppError::BadRequest(
            "A call is already active in this conversation".to_string(),
        ));
    }

    let now = Utc::now().to_rfc3339();
    let call_id = Uuid::new_v4().to_string();
    let open = payload.open;

    // Open calls start active immediately; private calls start ringing
    let initial_status = if open { "active" } else { "ringing" };

    let call = Call {
        call_id: call_id.clone(),
        conversation_id: id.clone(),
        initiated_by: auth.user.user_id.clone(),
        call_type: payload.call_type.clone(),
        status: initial_status.to_string(),
        started_at: if open { Some(now.clone()) } else { None },
        ended_at: None,
        duration_seconds: None,
        created_at: now.clone(),
        is_open: open as i32,
    };
    call_repo
        .create(&call)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create call: {}", e)))?;

    // Add initiator as joined participant
    let initiator = CallParticipant {
        id: Uuid::new_v4().to_string(),
        call_id: call_id.clone(),
        user_id: auth.user.user_id.clone(),
        role: "initiator".to_string(),
        status: "joined".to_string(),
        joined_at: Some(now.clone()),
        left_at: None,
    };
    call_repo
        .add_participant(&initiator)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to add initiator: {}", e)))?;

    let members = conv_repo
        .list_members(&id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list members: {}", e)))?;

    let caller_name = auth
        .user
        .display_name
        .clone()
        .unwrap_or_else(|| auth.user.username.clone());

    let mut participant_responses = vec![CallParticipantResponse {
        user_id: auth.user.user_id.clone(),
        display_name: Some(caller_name.clone()),
        role: "initiator".to_string(),
        status: "joined".to_string(),
    }];

    if open {
        // Open call: notify all conv members a channel is available, no ringing participants
        let available_msg = serde_json::json!({
        "type": "open_call_available",
        "call_id": call_id,
        "conversation_id": id,
        "call_type": payload.call_type,
        "host_id": auth.user.user_id,
        "host_name": caller_name,
        });
        let msg_str = available_msg.to_string();
        for member in &members {
            if member.user_id != auth.user.user_id {
                send_to_user(&registry, &member.user_id, &msg_str).await;
            }
        }
    } else {
        // Private call: add all members as ringing participants and ring them
        for member in &members {
            if member.user_id == auth.user.user_id {
                continue;
            }

            let participant = CallParticipant {
                id: Uuid::new_v4().to_string(),
                call_id: call_id.clone(),
                user_id: member.user_id.clone(),
                role: "participant".to_string(),
                status: "ringing".to_string(),
                joined_at: None,
                left_at: None,
            };
            if let Err(e) = call_repo.add_participant(&participant).await {
                tracing::error!(
                    "Failed to add participant {} to call {}: {}",
                    member.user_id,
                    call_id,
                    e
                );
            }

            let user = user_repo.find_by_id(&member.user_id).await.ok().flatten();
            let display_name = user.and_then(|u| u.display_name);

            participant_responses.push(CallParticipantResponse {
                user_id: member.user_id.clone(),
                display_name,
                role: "participant".to_string(),
                status: "ringing".to_string(),
            });

            let incoming_msg = serde_json::json!({
            "type": "call_incoming",
            "call_id": call_id,
            "conversation_id": id,
            "call_type": payload.call_type,
            "caller_id": auth.user.user_id,
            "caller_name": caller_name,
            });
            send_to_user(&registry, &member.user_id, &incoming_msg.to_string()).await;
        }

        // Spawn ring timeout for private calls only
        let timeout_pool = pool;
        let timeout_registry = registry;
        let timeout_call_id = call_id.clone();
        let timeout_user_id = auth.user.user_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            let call_repo = CallRepository::new(timeout_pool);
            if let Ok(Some(c)) = call_repo.get_by_id(&timeout_call_id).await {
                if c.status == "ringing" {
                    let _ = call_repo.update_status(&timeout_call_id, "missed").await;
                    if let Ok(participants) = call_repo.list_participants(&timeout_call_id).await {
                        for p in &participants {
                            if p.status == "ringing" {
                                let _ = call_repo
                                    .update_participant_status(
                                        &timeout_call_id,
                                        &p.user_id,
                                        "missed",
                                    )
                                    .await;
                            }
                        }
                        let ended_msg = serde_json::json!({
                        "type": "call_ended",
                        "call_id": timeout_call_id,
                        "reason": "timeout",
                        });
                        let msg_str = ended_msg.to_string();
                        for p in &participants {
                            send_to_user(&timeout_registry, &p.user_id, &msg_str).await;
                        }
                        send_to_user(&timeout_registry, &timeout_user_id, &msg_str).await;
                    }
                }
            }
        });
    }

    Ok(Json(CallResponse {
        call_id,
        conversation_id: id,
        call_type: payload.call_type,
        status: initial_status.to_string(),
        initiated_by: auth.user.user_id,
        participants: participant_responses,
        created_at: now,
    }))
}

// Helpers

async fn build_call_history_entry(
    call: &Call,
    call_repo: &CallRepository,
    user_repo: &UserRepository,
    conv_repo: &ConversationRepository,
) -> CallHistoryEntry {
    // Get conversation name
    let conversation_name = conv_repo
        .get_by_id(&call.conversation_id)
        .await
        .ok()
        .flatten()
        .and_then(|c| c.name);

    // Get initiator name
    let initiator_name = user_repo
        .find_by_id(&call.initiated_by)
        .await
        .ok()
        .flatten()
        .and_then(|u| u.display_name);

    // Get participants
    let participants = call_repo
        .list_participants(&call.call_id)
        .await
        .unwrap_or_default();

    let mut participant_responses = Vec::new();
    for p in &participants {
        let display_name = user_repo
            .find_by_id(&p.user_id)
            .await
            .ok()
            .flatten()
            .and_then(|u| u.display_name);
        participant_responses.push(CallParticipantResponse {
            user_id: p.user_id.clone(),
            display_name,
            role: p.role.clone(),
            status: p.status.clone(),
        });
    }

    CallHistoryEntry {
        call_id: call.call_id.clone(),
        conversation_id: call.conversation_id.clone(),
        conversation_name,
        call_type: call.call_type.clone(),
        status: call.status.clone(),
        initiated_by: call.initiated_by.clone(),
        initiator_name,
        participants: participant_responses,
        started_at: call.started_at.clone(),
        ended_at: call.ended_at.clone(),
        duration_seconds: call.duration_seconds,
        created_at: call.created_at.clone(),
    }
}
