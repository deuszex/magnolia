use axum::{
    Extension,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use sqlx::AnyPool;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::{RwLock, mpsc};
use uuid::Uuid;

use crate::config::Settings;
use magnolia_common::errors::AppError;
use magnolia_common::models::{Call, CallParticipant};
use magnolia_common::repositories::{
    CallRepository, ConversationRepository, GlobalCallRepository, UserRepository,
};
use magnolia_common::schemas::SignalMessage;

const SESSION_COOKIE_NAME: &str = "session_id";

/// A connected user's sender channel
pub struct UserConnection {
    pub conn_id: String,
    pub user_id: String,
    pub tx: mpsc::UnboundedSender<String>,
}

/// Global registry of connected users — supports multiple connections per user
/// (e.g. main tab + call tab each have their own WebSocket)
pub type ConnectionRegistry = Arc<RwLock<HashMap<String, Vec<UserConnection>>>>;

/// Create a new empty connection registry
pub fn new_registry() -> ConnectionRegistry {
    Arc::new(RwLock::new(HashMap::new()))
}

static USER_REGISTRY: OnceLock<ConnectionRegistry> = OnceLock::new();

/// Initialise the global user connection registry (called once at startup).
pub fn init_global_registry(registry: ConnectionRegistry) {
    let _ = USER_REGISTRY.set(registry);
}

/// Return the global user connection registry, if initialised.
pub fn global_user_registry() -> Option<&'static ConnectionRegistry> {
    USER_REGISTRY.get()
}

/// Send a JSON message to all connections of a specific user
pub async fn send_to_user(registry: &ConnectionRegistry, user_id: &str, msg: &str) {
    let reg = registry.read().await;
    if let Some(conns) = reg.get(user_id) {
        tracing::info!(
            "send_to_user: delivering to {} ({} connection(s))",
            user_id,
            conns.len()
        );
        for conn in conns {
            if conn.tx.send(msg.to_string()).is_err() {
                tracing::warn!(
                    "send_to_user: channel closed for {} (conn {})",
                    user_id,
                    conn.conn_id
                );
            }
        }
    } else {
        let connected_users: Vec<&String> = reg.keys().collect();
        tracing::warn!(
            "send_to_user: user {} NOT found in registry! Connected users: {:?}",
            user_id,
            connected_users
        );
    }
}

/// Send a call signal to `target_user_id`: locally if they are connected here,
/// or via S2S federation relay if they are on a remote server.
async fn send_to_user_or_federate(
    registry: &ConnectionRegistry,
    pool: &AnyPool,
    call_id: &str,
    target_user_id: &str,
    msg: &serde_json::Value,
) {
    let is_local = {
        let reg = registry.read().await;
        reg.contains_key(target_user_id)
    };
    if is_local {
        send_to_user(registry, target_user_id, &msg.to_string()).await;
        return;
    }
    // User not local — look up their federated server via the call's conversation.
    let call_repo = CallRepository::new(pool.clone());
    let conv_id = match call_repo.get_by_id(call_id).await {
        Ok(Some(c)) => c.conversation_id,
        _ => return,
    };
    if let Ok(fed_members) =
        crate::federation::repo::list_federated_members(pool, &conv_id).await
    {
        if let Some(fed) = fed_members
            .iter()
            .find(|m| m.remote_user_id == target_user_id)
        {
            let conn_id = fed.server_connection_id.clone();
            let pool_clone = pool.clone();
            let msg_clone = msg.clone();
            tokio::spawn(async move {
                crate::federation::hub::send_call_signal_to_peer(
                    &pool_clone,
                    &conn_id,
                    msg_clone,
                )
                .await;
            });
        }
    }
}

/// Check if a signaling message (SDP/ICE/key_exchange) belongs to a call owned by a remote
/// server. If so, relay the message to that server and return `true`.
/// Returns `false` when the call is local (or unknown) — caller should handle normally.
async fn try_relay_sdp_to_origin(
    pool: &AnyPool,
    call_id: &str,
    msg: serde_json::Value,
) -> bool {
    match CallRepository::new(pool.clone()).get_by_id(call_id).await {
        Ok(Some(_)) => return false, // Call exists locally — handle normally.
        Ok(None) => {}               // Call not found locally — may be federated.
        Err(_) => return false,
    }
    if let Some(origin_conn) =
        crate::federation::hub::get_federated_call_origin(call_id).await
    {
        let pool_clone = pool.clone();
        tokio::spawn(async move {
            crate::federation::hub::send_call_signal_to_peer(&pool_clone, &origin_conn, msg)
                .await;
        });
        return true;
    }
    false
}

/// WebSocket upgrade handler: GET /api/ws
/// Authenticates via session cookie before upgrading.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State((pool, _settings)): State<(AnyPool, Arc<Settings>)>,
    jar: CookieJar,
    Extension(registry): Extension<ConnectionRegistry>,
) -> Result<Response, AppError> {
    // Authenticate via session cookie (same logic as require_auth)
    let session_id = jar
        .get(SESSION_COOKIE_NAME)
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;

    let user_repo = UserRepository::new(pool.clone());
    let session = user_repo
        .find_session(&session_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if session.is_expired() {
        user_repo.delete_session(&session_id).await?;
        return Err(AppError::Unauthorized);
    }

    let user = user_repo
        .find_by_id(&session.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if user.active == 0 {
        return Err(AppError::Forbidden);
    }

    let user_id = user.user_id.clone();

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, user_id, pool, registry)))
}

/// Handle an authenticated WebSocket connection
async fn handle_socket(
    socket: WebSocket,
    user_id: String,
    pool: AnyPool,
    registry: ConnectionRegistry,
) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Create channel for outgoing messages
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // Unique ID for this specific connection
    let conn_id = Uuid::new_v4().to_string();

    // Register this connection (append to user's connection list)
    {
        let mut reg = registry.write().await;
        reg.entry(user_id.clone())
            .or_insert_with(Vec::new)
            .push(UserConnection {
                conn_id: conn_id.clone(),
                user_id: user_id.clone(),
                tx,
            });
    }

    tracing::info!("WebSocket connected: user={} (conn {})", user_id, conn_id);
    {
        let reg = registry.read().await;
        let total_conns: usize = reg.values().map(|v| v.len()).sum();
        tracing::info!(
            "WS registry now has {} users, {} total connections",
            reg.len(),
            total_conns
        );
    }

    // Spawn task to forward channel messages to the WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Spawn heartbeat ping task (pings this specific connection)
    let registry_clone = registry.clone();
    let user_id_clone = user_id.clone();
    let conn_id_clone = conn_id.clone();
    let heartbeat = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let reg = registry_clone.read().await;
            if let Some(conns) = reg.get(&user_id_clone) {
                if let Some(conn) = conns.iter().find(|c| c.conn_id == conn_id_clone) {
                    if conn.tx.send(r#"{"type":"ping"}"#.to_string()).is_err() {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    });

    // Receive loop: dispatch incoming messages
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let text_str: &str = &text;
                if let Err(e) = dispatch_signal(text_str, &user_id, &pool, &registry).await {
                    let error_msg = serde_json::json!({
                    "type": "error",
                    "message": format!("{:?}", e)
                    });
                    send_to_user(&registry, &user_id, &error_msg.to_string()).await;
                }
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    // Cleanup: remove this specific connection from registry
    let is_last_connection = {
        let mut reg = registry.write().await;
        if let Some(conns) = reg.get_mut(&user_id) {
            conns.retain(|c| c.conn_id != conn_id);
            if conns.is_empty() {
                reg.remove(&user_id);
                true
            } else {
                false
            }
        } else {
            false
        }
    };

    // If this was the user's last connection, remove them from the global call
    if is_last_connection {
        let repo = GlobalCallRepository::new(pool.clone());
        if let Ok(true) = repo.is_participant(&user_id).await {
            let _ = repo.leave(&user_id).await;
            let _ =
                crate::handlers::global_call::broadcast_global_call_update(&pool, &registry).await;
        }
    }

    heartbeat.abort();
    send_task.abort();

    tracing::info!("WebSocket disconnected: {} (conn {})", user_id, conn_id);
}

// Maximum accepted payload sizes to prevent memory exhaustion
const MAX_SDP_LEN: usize = 16 * 1024; // 16 KB
const MAX_ICE_LEN: usize = 4 * 1024; // 4 KB
const MAX_KEY_LEN: usize = 256; // base64 X25519 public key

/// Parse incoming JSON and route to appropriate handler
async fn dispatch_signal(
    msg: &str,
    user_id: &str,
    pool: &AnyPool,
    registry: &ConnectionRegistry,
) -> Result<(), AppError> {
    let signal: SignalMessage = serde_json::from_str(msg)
        .map_err(|e| AppError::BadRequest(format!("Invalid signal message: {}", e)))?;

    // Log message type only — never log SDP/ICE/key content
    tracing::info!("WS signal from {}: {}", user_id, signal.type_name());

    match signal {
        SignalMessage::CallInitiate {
            conversation_id,
            call_type,
            open,
        } => {
            handle_call_initiate(user_id, &conversation_id, &call_type, open, pool, registry).await
        }
        SignalMessage::CallAccept { call_id } => {
            handle_call_accept(user_id, &call_id, pool, registry).await
        }
        SignalMessage::CallReject { call_id } => {
            handle_call_reject(user_id, &call_id, pool, registry).await
        }
        SignalMessage::CallHangup { call_id } => {
            handle_call_hangup(user_id, &call_id, pool, registry).await
        }
        SignalMessage::CallBusy { call_id } => {
            handle_call_reject(user_id, &call_id, pool, registry).await
        }
        SignalMessage::CallJoin { call_id } => {
            handle_call_join(user_id, &call_id, pool, registry).await
        }
        SignalMessage::IceCandidate {
            call_id,
            target_user_id,
            candidate,
        } => {
            // Validate serialized candidate size
            let candidate_str = candidate.to_string();
            if candidate_str.len() > MAX_ICE_LEN {
                return Err(AppError::BadRequest(
                    "ICE candidate payload too large".to_string(),
                ));
            }
            let msg = serde_json::json!({
            "type": "ice_candidate",
            "call_id": &call_id,
            "from_user_id": user_id,
            "target_user_id": &target_user_id,
            "candidate": candidate,
            });
            if try_relay_sdp_to_origin(pool, &call_id, msg.clone()).await {
                return Ok(());
            }
            verify_signaling_auth(pool, &call_id, user_id, &target_user_id).await?;
            send_to_user_or_federate(registry, pool, &call_id, &target_user_id, &msg).await;
            Ok(())
        }
        SignalMessage::SdpOffer {
            call_id,
            target_user_id,
            sdp,
        } => {
            if sdp.len() > MAX_SDP_LEN {
                return Err(AppError::BadRequest("SDP payload too large".to_string()));
            }
            let msg = serde_json::json!({
            "type": "sdp_offer",
            "call_id": &call_id,
            "from_user_id": user_id,
            "target_user_id": &target_user_id,
            "sdp": sdp,
            });
            if try_relay_sdp_to_origin(pool, &call_id, msg.clone()).await {
                return Ok(());
            }
            verify_signaling_auth(pool, &call_id, user_id, &target_user_id).await?;
            send_to_user_or_federate(registry, pool, &call_id, &target_user_id, &msg).await;
            Ok(())
        }
        SignalMessage::SdpAnswer {
            call_id,
            target_user_id,
            sdp,
        } => {
            if sdp.len() > MAX_SDP_LEN {
                return Err(AppError::BadRequest("SDP payload too large".to_string()));
            }
            let msg = serde_json::json!({
            "type": "sdp_answer",
            "call_id": &call_id,
            "from_user_id": user_id,
            "target_user_id": &target_user_id,
            "sdp": sdp,
            });
            if try_relay_sdp_to_origin(pool, &call_id, msg.clone()).await {
                return Ok(());
            }
            verify_signaling_auth(pool, &call_id, user_id, &target_user_id).await?;
            send_to_user_or_federate(registry, pool, &call_id, &target_user_id, &msg).await;
            Ok(())
        }
        SignalMessage::KeyExchange {
            call_id,
            target_user_id,
            public_key,
        } => {
            if public_key.len() > MAX_KEY_LEN {
                return Err(AppError::BadRequest("Key payload too large".to_string()));
            }
            let msg = serde_json::json!({
            "type": "key_exchange",
            "call_id": &call_id,
            "from_user_id": user_id,
            "target_user_id": &target_user_id,
            "public_key": public_key,
            });
            if try_relay_sdp_to_origin(pool, &call_id, msg.clone()).await {
                return Ok(());
            }
            verify_signaling_auth(pool, &call_id, user_id, &target_user_id).await?;
            send_to_user_or_federate(registry, pool, &call_id, &target_user_id, &msg).await;
            Ok(())
        }
        SignalMessage::CallKick {
            call_id,
            target_user_id,
        } => handle_call_kick(user_id, &call_id, &target_user_id, pool, registry).await,
        SignalMessage::GlobalCallOffer {
            target_user_id,
            sdp,
        } => {
            if sdp.len() > MAX_SDP_LEN {
                return Err(AppError::BadRequest("SDP payload too large".to_string()));
            }
            verify_global_call_signaling_auth(pool, user_id, &target_user_id).await?;
            let msg = serde_json::json!({
            "type": "global_call_offer",
            "from_user_id": user_id,
            "sdp": sdp,
            });
            send_to_user(registry, &target_user_id, &msg.to_string()).await;
            Ok(())
        }
        SignalMessage::GlobalCallAnswer {
            target_user_id,
            sdp,
        } => {
            if sdp.len() > MAX_SDP_LEN {
                return Err(AppError::BadRequest("SDP payload too large".to_string()));
            }
            verify_global_call_signaling_auth(pool, user_id, &target_user_id).await?;
            let msg = serde_json::json!({
            "type": "global_call_answer",
            "from_user_id": user_id,
            "sdp": sdp,
            });
            send_to_user(registry, &target_user_id, &msg.to_string()).await;
            Ok(())
        }
        SignalMessage::GlobalCallIce {
            target_user_id,
            candidate,
        } => {
            let candidate_str = candidate.to_string();
            if candidate_str.len() > MAX_ICE_LEN {
                return Err(AppError::BadRequest(
                    "ICE candidate payload too large".to_string(),
                ));
            }
            verify_global_call_signaling_auth(pool, user_id, &target_user_id).await?;
            let msg = serde_json::json!({
            "type": "global_call_ice",
            "from_user_id": user_id,
            "candidate": candidate,
            });
            send_to_user(registry, &target_user_id, &msg.to_string()).await;
            Ok(())
        }
        SignalMessage::Ping => {
            send_to_user(registry, user_id, r#"{"type":"pong"}"#).await;
            Ok(())
        }
        SignalMessage::Pong => Ok(()),
    }
}

/// Verify that both sender and target are currently in the global call.
async fn verify_global_call_signaling_auth(
    pool: &AnyPool,
    sender_id: &str,
    target_id: &str,
) -> Result<(), AppError> {
    let repo = GlobalCallRepository::new(pool.clone());
    if !repo
        .is_participant(sender_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    {
        return Err(AppError::Forbidden);
    }
    if !repo
        .is_participant(target_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

/// Verify that the sender is a joined participant and the target is reachable.
/// The target may be a local joined participant or a known federated member of
/// the call's conversation (remote users are never in local call_participants
/// because the FK constraint prevents inserting non-local user IDs).
async fn verify_signaling_auth(
    pool: &AnyPool,
    call_id: &str,
    sender_id: &str,
    target_id: &str,
) -> Result<(), AppError> {
    let call_repo = CallRepository::new(pool.clone());

    let call = call_repo
        .get_by_id(call_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get call: {}", e)))?
        .ok_or_else(|| AppError::NotFound("Call not found".to_string()))?;

    if call.status != "active" {
        return Err(AppError::BadRequest("Call is not active".to_string()));
    }

    // Sender must be a locally-joined participant.
    let sender = call_repo
        .get_participant(call_id, sender_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get participant: {}", e)))?
        .ok_or(AppError::Forbidden)?;

    if sender.status != "joined" {
        return Err(AppError::Forbidden);
    }

    // Target: accept if they are a local joined participant OR a known federated
    // member of the call's conversation (remote users can't be in call_participants
    // due to the FK constraint on user_accounts).
    let target = call_repo
        .get_participant(call_id, target_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get participant: {}", e)))?;

    match target {
        Some(p) if p.status == "joined" => {} // local joined participant ✓
        Some(_) => return Err(AppError::Forbidden), // local but not joined
        None => {
            // Not a local participant — check federated conversation members.
            let fed_members =
                crate::federation::repo::list_federated_members(pool, &call.conversation_id)
                    .await
                    .unwrap_or_default();
            if !fed_members.iter().any(|m| m.remote_user_id == target_id) {
                return Err(AppError::Forbidden);
            }
            // Known federated user — allow.
        }
    }

    Ok(())
}

// Signal Handlers

async fn handle_call_initiate(
    user_id: &str,
    conversation_id: &str,
    call_type: &str,
    open: bool,
    pool: &AnyPool,
    registry: &ConnectionRegistry,
) -> Result<(), AppError> {
    let conv_repo = ConversationRepository::new(pool.clone());
    let call_repo = CallRepository::new(pool.clone());
    let user_repo = UserRepository::new(pool.clone());

    // Verify user is member of conversation
    if !conv_repo
        .is_member(conversation_id, user_id)
        .await
        .unwrap_or(false)
    {
        return Err(AppError::Forbidden);
    }

    // Check for existing active call
    if call_repo
        .get_active_call(conversation_id)
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

    // Open calls start active immediately; private calls start ringing
    let initial_status = if open { "active" } else { "ringing" };

    let call = Call {
        call_id: call_id.clone(),
        conversation_id: conversation_id.to_string(),
        initiated_by: user_id.to_string(),
        call_type: call_type.to_string(),
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
    let initiator_participant = CallParticipant {
        id: Uuid::new_v4().to_string(),
        call_id: call_id.clone(),
        user_id: user_id.to_string(),
        role: "initiator".to_string(),
        status: "joined".to_string(),
        joined_at: Some(now.clone()),
        left_at: None,
    };
    call_repo
        .add_participant(&initiator_participant)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to add initiator: {}", e)))?;

    let caller = user_repo.find_by_id(user_id).await.ok().flatten();
    let caller_name = caller
        .as_ref()
        .and_then(|u| u.display_name.clone())
        .unwrap_or_else(|| {
            caller
                .as_ref()
                .map(|u| u.username.clone())
                .unwrap_or_default()
        });

    // Get conversation members (excluding initiator)
    let members = conv_repo
        .list_members(conversation_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list members: {}", e)))?;

    if open {
        // Open call: notify all conv members that a channel is available to join
        let available_msg = serde_json::json!({
        "type": "open_call_available",
        "call_id": call_id,
        "conversation_id": conversation_id,
        "call_type": call_type,
        "host_id": user_id,
        "host_name": caller_name,
        });
        let msg_str = available_msg.to_string();
        for member in &members {
            if member.user_id != user_id {
                send_to_user(registry, &member.user_id, &msg_str).await;
            }
        }
        // Relay open-call notification to federated members too.
        if let Ok(fed_members) =
            crate::federation::repo::list_federated_members(pool, conversation_id).await
        {
            for fed_member in &fed_members {
                let relay_payload = serde_json::json!({
                "type": "open_call_available",
                "call_id": call_id,
                "conversation_id": conversation_id,
                "call_type": call_type,
                "host_id": user_id,
                "host_name": caller_name,
                "recipient_user_id": fed_member.remote_user_id,
                });
                let conn_id = fed_member.server_connection_id.clone();
                let pool_clone = pool.clone();
                tokio::spawn(async move {
                    crate::federation::hub::send_call_signal_to_peer(
                        &pool_clone,
                        &conn_id,
                        relay_payload,
                    )
                    .await;
                });
            }
        }
    } else {
        // Private call: add all members as ringing participants and ring them
        for member in &members {
            if member.user_id == user_id {
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

            let incoming_msg = serde_json::json!({
            "type": "call_incoming",
            "call_id": call_id,
            "conversation_id": conversation_id,
            "call_type": call_type,
            "caller_id": user_id,
            "caller_name": caller_name,
            });
            send_to_user(registry, &member.user_id, &incoming_msg.to_string()).await;
        }

        // Relay to federated members of this conversation.
        // Note: we do NOT add proxy call_participants for remote users because
        // call_participants.user_id has a FK to user_accounts — remote user IDs
        // don't exist locally. verify_signaling_auth handles federated targets by
        // checking federated_conversation_members directly.
        if let Ok(fed_members) =
            crate::federation::repo::list_federated_members(pool, conversation_id).await
        {
            for fed_member in &fed_members {
                let relay_payload = serde_json::json!({
                "type": "call_incoming",
                "call_id": call_id,
                "conversation_id": conversation_id,
                "call_type": call_type,
                "caller_id": user_id,
                "caller_name": caller_name,
                "recipient_user_id": fed_member.remote_user_id,
                });
                let conn_id = fed_member.server_connection_id.clone();
                let pool_clone = pool.clone();
                tokio::spawn(async move {
                    crate::federation::hub::send_call_signal_to_peer(
                        &pool_clone,
                        &conn_id,
                        relay_payload,
                    )
                    .await;
                });
            }
        }

        // Spawn ring timeout (30 seconds) for private calls only
        let timeout_pool = pool.clone();
        let timeout_registry = registry.clone();
        let timeout_call_id = call_id.clone();
        let timeout_user_id = user_id.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

            let call_repo = CallRepository::new(timeout_pool.clone());
            if let Ok(Some(call)) = call_repo.get_by_id(&timeout_call_id).await {
                if call.status == "ringing" {
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
                    }

                    let ended_msg = serde_json::json!({
                    "type": "call_ended",
                    "call_id": timeout_call_id,
                    "reason": "timeout",
                    });
                    send_to_user(&timeout_registry, &timeout_user_id, &ended_msg.to_string()).await;
                }
            }
        });
    }

    Ok(())
}

async fn handle_call_accept(
    user_id: &str,
    call_id: &str,
    pool: &AnyPool,
    registry: &ConnectionRegistry,
) -> Result<(), AppError> {
    let call_repo = CallRepository::new(pool.clone());

    let call_opt = call_repo
        .get_by_id(call_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get call: {}", e)))?;

    // If the call record doesn't exist locally it may be owned by a remote server.
    if call_opt.is_none() {
        if let Some(origin_conn) = crate::federation::hub::get_federated_call_origin(call_id).await
        {
            let payload = serde_json::json!({
            "type": "call_accept",
            "call_id": call_id,
            "user_id": user_id,
            });
            let pool_clone = pool.clone();
            tokio::spawn(async move {
                crate::federation::hub::send_call_signal_to_peer(
                    &pool_clone,
                    &origin_conn,
                    payload,
                )
                .await;
            });
            // Do NOT send call_accepted to the receiver here — it would trigger
            // createAndSendOffer with their own user_id as the target (they would
            // peer-connect to themselves). The receiver just waits for the initiator's
            // SDP offer, which arrives after the origin server processes call_accept.
            return Ok(());
        }
        return Err(AppError::NotFound("Call not found".to_string()));
    }
    let call = call_opt.unwrap();

    // Open calls don't have an accept flow — use CallJoin instead
    if call.is_open != 0 {
        return Err(AppError::BadRequest(
            "Use call_join for open calls".to_string(),
        ));
    }

    if call.status != "ringing" && call.status != "active" {
        return Err(AppError::BadRequest("Call is not active".to_string()));
    }

    // Verify this user was actually invited (must have a "ringing" participant record)
    let participant = call_repo
        .get_participant(call_id, user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get participant: {}", e)))?
        .ok_or(AppError::Forbidden)?;

    if participant.status != "ringing" {
        return Err(AppError::Forbidden);
    }

    let now = Utc::now().to_rfc3339();

    // Mark participant as joined
    call_repo
        .set_participant_joined(call_id, user_id, &now)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to update participant: {}", e)))?;

    // If this is the first accept, mark call as active
    if call.status == "ringing" {
        call_repo
            .set_started(call_id, &now)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to start call: {}", e)))?;
    }

    // Notify all participants
    let participants = call_repo
        .list_participants(call_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list participants: {}", e)))?;

    tracing::info!(
        "Call {} has {} participants: {:?}",
        call_id,
        participants.len(),
        participants
            .iter()
            .map(|p| format!("{}({})", p.user_id, p.status))
            .collect::<Vec<_>>()
    );

    let accepted_msg = serde_json::json!({
    "type": "call_accepted",
    "call_id": call_id,
    "user_id": user_id,
    });
    let msg_str = accepted_msg.to_string();
    for p in &participants {
        if p.user_id != user_id {
            tracing::info!("Sending call_accepted to user {}", p.user_id);
            send_to_user(registry, &p.user_id, &msg_str).await;
        }
    }

    Ok(())
}

async fn handle_call_reject(
    user_id: &str,
    call_id: &str,
    pool: &AnyPool,
    registry: &ConnectionRegistry,
) -> Result<(), AppError> {
    let call_repo = CallRepository::new(pool.clone());

    let call_opt = call_repo
        .get_by_id(call_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get call: {}", e)))?;

    if call_opt.is_none() {
        if let Some(origin_conn) = crate::federation::hub::get_federated_call_origin(call_id).await
        {
            let payload = serde_json::json!({
            "type": "call_reject",
            "call_id": call_id,
            "user_id": user_id,
            });
            let pool_clone = pool.clone();
            tokio::spawn(async move {
                crate::federation::hub::send_call_signal_to_peer(
                    &pool_clone,
                    &origin_conn,
                    payload,
                )
                .await;
            });
            return Ok(());
        }
        return Err(AppError::NotFound("Call not found".to_string()));
    }
    let call = call_opt.unwrap();

    // Open calls don't have a reject flow
    if call.is_open != 0 {
        return Err(AppError::BadRequest(
            "Cannot reject an open call".to_string(),
        ));
    }

    // Verify this user was actually invited
    let participant = call_repo
        .get_participant(call_id, user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get participant: {}", e)))?
        .ok_or(AppError::Forbidden)?;

    if participant.status != "ringing" {
        return Err(AppError::Forbidden);
    }

    // Mark participant as rejected
    call_repo
        .update_participant_status(call_id, user_id, "rejected")
        .await
        .map_err(|e| AppError::Internal(format!("Failed to update participant: {}", e)))?;

    // Notify all participants
    let participants = call_repo
        .list_participants(call_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list participants: {}", e)))?;

    let rejected_msg = serde_json::json!({
    "type": "call_rejected",
    "call_id": call_id,
    "user_id": user_id,
    });
    let msg_str = rejected_msg.to_string();
    for p in &participants {
        if p.user_id != user_id {
            send_to_user(registry, &p.user_id, &msg_str).await;
        }
    }

    // Check if all non-initiator participants have rejected/missed
    if call.status == "ringing" {
        if let Ok(true) = call_repo.all_participants_declined(call_id).await {
            let _ = call_repo.update_status(call_id, "rejected").await;
            let ended_msg = serde_json::json!({
            "type": "call_ended",
            "call_id": call_id,
            "reason": "rejected",
            });
            // Notify initiator
            send_to_user(registry, &call.initiated_by, &ended_msg.to_string()).await;
        }
    }

    Ok(())
}

async fn handle_call_hangup(
    user_id: &str,
    call_id: &str,
    pool: &AnyPool,
    registry: &ConnectionRegistry,
) -> Result<(), AppError> {
    let call_repo = CallRepository::new(pool.clone());

    let call_opt = call_repo
        .get_by_id(call_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get call: {}", e)))?;

    // If the call record doesn't exist locally it may be owned by a remote server.
    if call_opt.is_none() {
        if let Some(origin_conn) =
            crate::federation::hub::get_federated_call_origin(call_id).await
        {
            let payload = serde_json::json!({
                "type": "call_hangup",
                "call_id": call_id,
                "user_id": user_id,
            });
            let pool_clone = pool.clone();
            tokio::spawn(async move {
                crate::federation::hub::send_call_signal_to_peer(
                    &pool_clone,
                    &origin_conn,
                    payload,
                )
                .await;
            });
            return Ok(());
        }
        return Err(AppError::NotFound("Call not found".to_string()));
    }
    let call = call_opt.unwrap();

    // Verify user is a participant in this call
    call_repo
        .get_participant(call_id, user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get participant: {}", e)))?
        .ok_or(AppError::Forbidden)?;

    let now = Utc::now().to_rfc3339();

    // Mark participant as left
    call_repo
        .set_participant_left(call_id, user_id, &now)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to update participant: {}", e)))?;

    // Check remaining active participants
    let active_count = call_repo
        .count_active_participants(call_id)
        .await
        .unwrap_or(0);

    let participants = call_repo
        .list_participants(call_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to list participants: {}", e)))?;

    if active_count <= 1 {
        // Call is over — end it
        let duration = if let Some(ref started) = call.started_at {
            if let Ok(start_time) = chrono::DateTime::parse_from_rfc3339(started) {
                let end_time = Utc::now();
                (end_time - start_time.with_timezone(&Utc)).num_seconds() as i32
            } else {
                0
            }
        } else {
            0
        };

        let _ = call_repo.set_ended(call_id, &now, duration).await;

        // Mark any remaining joined participants as left
        for p in &participants {
            if p.status == "joined" && p.user_id != user_id {
                let _ = call_repo
                    .set_participant_left(call_id, &p.user_id, &now)
                    .await;
            }
            if p.status == "ringing" {
                let _ = call_repo
                    .update_participant_status(call_id, &p.user_id, "missed")
                    .await;
            }
        }

        // Notify everyone (local participants)
        let ended_msg = serde_json::json!({
        "type": "call_ended",
        "call_id": call_id,
        "reason": "hangup",
        });
        let msg_str = ended_msg.to_string();
        for p in &participants {
            send_to_user(registry, &p.user_id, &msg_str).await;
        }

        // Relay call_ended to any federated participants on remote servers.
        if let Ok(fed_members) =
            crate::federation::repo::list_federated_members(pool, &call.conversation_id).await
        {
            for fed_member in fed_members {
                let relay_ended = serde_json::json!({
                    "type": "call_ended",
                    "call_id": call_id,
                    "reason": "hangup",
                    "recipient_user_id": fed_member.remote_user_id,
                });
                let conn_id = fed_member.server_connection_id.clone();
                let pool_clone = pool.clone();
                tokio::spawn(async move {
                    crate::federation::hub::send_call_signal_to_peer(
                        &pool_clone,
                        &conn_id,
                        relay_ended,
                    )
                    .await;
                });
            }
        }
    } else {
        // Participant left but call continues
        let left_msg = serde_json::json!({
        "type": "participant_left",
        "call_id": call_id,
        "user_id": user_id,
        });
        let msg_str = left_msg.to_string();
        for p in &participants {
            if p.user_id != user_id {
                send_to_user(registry, &p.user_id, &msg_str).await;
            }
        }
    }

    Ok(())
}

async fn handle_call_kick(
    user_id: &str,
    call_id: &str,
    target_user_id: &str,
    pool: &AnyPool,
    registry: &ConnectionRegistry,
) -> Result<(), AppError> {
    let call_repo = CallRepository::new(pool.clone());

    let call = call_repo
        .get_by_id(call_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get call: {}", e)))?
        .ok_or(AppError::NotFound("Call not found".to_string()))?;

    if call.status != "active" {
        return Err(AppError::BadRequest("Call is not active".to_string()));
    }

    // Only the initiator may kick participants
    if call.initiated_by != user_id {
        return Err(AppError::Forbidden);
    }

    // Target must be a joined participant
    let target = call_repo
        .get_participant(call_id, target_user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get participant: {}", e)))?
        .ok_or_else(|| AppError::NotFound("Participant not found".to_string()))?;

    if target.status != "joined" {
        return Err(AppError::BadRequest(
            "Participant is not in the call".to_string(),
        ));
    }

    let now = Utc::now().to_rfc3339();

    // Mark kicked participant as left
    call_repo
        .set_participant_left(call_id, target_user_id, &now)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to update participant: {}", e)))?;

    // Tell the kicked user they've been removed
    let kicked_msg = serde_json::json!({
    "type": "call_kicked",
    "call_id": call_id,
    });
    send_to_user(registry, target_user_id, &kicked_msg.to_string()).await;

    // Tell remaining participants someone left
    let participants = call_repo
        .list_participants(call_id)
        .await
        .unwrap_or_default();
    let left_msg = serde_json::json!({
    "type": "participant_left",
    "call_id": call_id,
    "user_id": target_user_id,
    });
    let msg_str = left_msg.to_string();
    for p in &participants {
        if p.user_id != target_user_id && p.user_id != user_id && p.status == "joined" {
            send_to_user(registry, &p.user_id, &msg_str).await;
        }
    }
    // Also notify initiator so their UI updates
    send_to_user(registry, user_id, &msg_str).await;

    Ok(())
}

/// Called by hub.rs when a remote server relays a call signal back to us
/// (e.g. call_accept, call_reject, sdp_offer, ice_candidate, call_ended, …).
///
/// The `conn_id` is the server_connection_id that sent the signal.
/// The payload must contain a `"type"` field identifying the signal kind.
pub async fn relay_federated_call_signal(
    pool: &AnyPool,
    _conn_id: &str,
    payload: serde_json::Value,
) {
    let type_str = match payload.get("type").and_then(|v| v.as_str()) {
        Some(t) => t.to_string(),
        None => return,
    };

    let Some(registry) = global_user_registry() else {
        return;
    };

    match type_str.as_str() {
        // A remote user accepted a call initiated here — forward to local participants.
        "call_accept" | "call_accepted" => {
            let call_id = match payload.get("call_id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => return,
            };
            let call_repo = CallRepository::new(pool.clone());
            let now = Utc::now().to_rfc3339();

            // Mark call as active if still ringing.
            if let Ok(Some(call)) = call_repo.get_by_id(&call_id).await {
                if call.status == "ringing" {
                    let _ = call_repo.set_started(&call_id, &now).await;
                }
            }

            // Notify local participants with the correct call_accepted type.
            // The raw payload type is "call_accept" which the frontend doesn't handle.
            let remote_uid = payload
                .get("user_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let accepted_msg = serde_json::json!({
                "type": "call_accepted",
                "call_id": &call_id,
                "user_id": remote_uid,
            });
            if let Ok(participants) = call_repo.list_participants(&call_id).await {
                let msg = accepted_msg.to_string();
                for p in &participants {
                    // Skip the proxy participant — they're on the remote server.
                    if p.user_id != remote_uid {
                        send_to_user(registry, &p.user_id, &msg).await;
                    }
                }
            }
        }

        // A remote participant hung up a call owned by this server.
        "call_hangup" => {
            let call_id = match payload.get("call_id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => return,
            };
            let remote_uid = payload
                .get("user_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let call_repo = CallRepository::new(pool.clone());
            let now = Utc::now().to_rfc3339();

            if !remote_uid.is_empty() {
                let _ = call_repo
                    .set_participant_left(&call_id, &remote_uid, &now)
                    .await;
            }

            let active_count = call_repo
                .count_active_participants(&call_id)
                .await
                .unwrap_or(0);
            let participants = call_repo
                .list_participants(&call_id)
                .await
                .unwrap_or_default();

            if active_count <= 1 {
                let _ = call_repo.set_ended(&call_id, &now, 0).await;
                crate::federation::hub::remove_federated_call(&call_id).await;
                let ended_msg = serde_json::json!({
                    "type": "call_ended",
                    "call_id": &call_id,
                    "reason": "hangup",
                });
                let msg_str = ended_msg.to_string();
                for p in &participants {
                    send_to_user(registry, &p.user_id, &msg_str).await;
                }
            } else {
                let left_msg = serde_json::json!({
                    "type": "participant_left",
                    "call_id": &call_id,
                    "user_id": &remote_uid,
                });
                let msg_str = left_msg.to_string();
                for p in &participants {
                    if p.user_id != remote_uid {
                        send_to_user(registry, &p.user_id, &msg_str).await;
                    }
                }
            }
        }

        // A remote user rejected the call — forward to local participants.
        "call_reject" | "call_rejected" => {
            let call_id = match payload.get("call_id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => return,
            };
            let call_repo = CallRepository::new(pool.clone());
            if let Ok(participants) = call_repo.list_participants(&call_id).await {
                let msg = payload.to_string();
                for p in &participants {
                    send_to_user(registry, &p.user_id, &msg).await;
                }
            }
        }

        // Call ended on the remote side — forward to local participants and clean up relay.
        "call_ended" => {
            let call_id = match payload.get("call_id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => return,
            };
            crate::federation::hub::remove_federated_call(&call_id).await;
            let call_repo = CallRepository::new(pool.clone());
            if let Ok(participants) = call_repo.list_participants(&call_id).await {
                let msg = payload.to_string();
                for p in &participants {
                    send_to_user(registry, &p.user_id, &msg).await;
                }
            }
        }

        // SDP / ICE / key-exchange: forward to the specific target user.
        // The target may be local (normal case) or on a remote server (federated relay).
        // Spawn to avoid deepening the hub inbound event chain.
        "sdp_offer" | "sdp_answer" | "ice_candidate" | "key_exchange" => {
            let call_id = match payload.get("call_id").and_then(|v| v.as_str()) {
                Some(id) => id.to_string(),
                None => return,
            };
            let target = match payload.get("target_user_id").and_then(|v| v.as_str()) {
                Some(t) => t.to_string(),
                None => return,
            };
            if let Some(reg) = global_user_registry() {
                // Try local delivery first — no DB needed, no chain depth added.
                let is_local = reg.read().await.contains_key(&target);
                if is_local {
                    send_to_user(reg, &target, &payload.to_string()).await;
                    return;
                }
            }
            // Remote target: spawn to avoid inline DB query deepening this chain.
            let pool_clone = pool.clone();
            let payload_clone = payload.clone();
            tokio::spawn(async move {
                if let Some(reg) = global_user_registry() {
                    send_to_user_or_federate(reg, &pool_clone, &call_id, &target, &payload_clone).await;
                }
            });
        }

        // Unknown or unhandled signal type — ignore.
        _ => {
            tracing::debug!("relay_federated_call_signal: unhandled type '{}'", type_str);
        }
    }
}

async fn handle_call_join(
    user_id: &str,
    call_id: &str,
    pool: &AnyPool,
    registry: &ConnectionRegistry,
) -> Result<(), AppError> {
    let call_repo = CallRepository::new(pool.clone());
    let conv_repo = ConversationRepository::new(pool.clone());
    let user_repo = UserRepository::new(pool.clone());

    let call = call_repo
        .get_by_id(call_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get call: {}", e)))?
        .ok_or(AppError::NotFound("Call not found".to_string()))?;

    if call.status != "active" {
        return Err(AppError::BadRequest("Call is not active".to_string()));
    }

    let now = Utc::now().to_rfc3339();
    let existing = call_repo
        .get_participant(call_id, user_id)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get participant: {}", e)))?;

    if call.is_open != 0 {
        // Open call: any conversation member may join
        if !conv_repo
            .is_member(&call.conversation_id, user_id)
            .await
            .unwrap_or(false)
        {
            return Err(AppError::Forbidden);
        }

        if existing.is_some() {
            call_repo
                .set_participant_joined(call_id, user_id, &now)
                .await
                .map_err(|e| AppError::Internal(format!("Failed to rejoin: {}", e)))?;
        } else {
            let participant = CallParticipant {
                id: Uuid::new_v4().to_string(),
                call_id: call_id.to_string(),
                user_id: user_id.to_string(),
                role: "participant".to_string(),
                status: "joined".to_string(),
                joined_at: Some(now.clone()),
                left_at: None,
            };
            call_repo
                .add_participant(&participant)
                .await
                .map_err(|e| AppError::Internal(format!("Failed to add participant: {}", e)))?;
        }
    } else {
        // Private call: user must have been invited (must already have a participant record)
        match existing {
            Some(_) => {
                call_repo
                    .set_participant_joined(call_id, user_id, &now)
                    .await
                    .map_err(|e| AppError::Internal(format!("Failed to rejoin: {}", e)))?;
            }
            None => return Err(AppError::Forbidden),
        }
    }

    // Get user display name
    let user = user_repo.find_by_id(user_id).await.ok().flatten();
    let user_name = user
        .as_ref()
        .and_then(|u| u.display_name.clone())
        .unwrap_or_else(|| {
            user.as_ref()
                .map(|u| u.username.clone())
                .unwrap_or_default()
        });

    // Fetch current participants to notify them
    let participants = call_repo
        .list_participants(call_id)
        .await
        .unwrap_or_default();

    // Notify existing participants
    let joined_msg = serde_json::json!({
    "type": "participant_joined",
    "call_id": call_id,
    "user_id": user_id,
    "user_name": user_name,
    });
    let msg_str = joined_msg.to_string();
    for p in &participants {
        if p.user_id != user_id && p.status == "joined" {
            send_to_user(registry, &p.user_id, &msg_str).await;
        }
    }

    Ok(())
}
