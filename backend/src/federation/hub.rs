//! Persistent WebSocket hub for server-to-server real-time events.
//!
//! ## Reconnect policy
//!
//! Each active peer has one **outbound** Tokio task. On disconnect the task
//! retries with a stepped back-off based on how long the peer has been
//! unreachable:
//!
//! | Elapsed since first failure | Retry interval |
//! |-----------------------------|----------------|
//! | 0 - 1 min | 10 s |
//! | 1 - 11 min | 1 min |
//! | 11 min - 24 h | 15 min |
//! | >= 24 h | give up (Gone) |
//!
//! When a peer sends `going_offline` before shutting down cleanly the outbound
//! task exits immediately (PeerOffline state). When that peer restarts it
//! connects inbound to us; handle_peer_ws detects the authenticated Hello and
//! re-spawns our outbound task toward them.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use axum::extract::ws::{Message as AxMessage, WebSocket};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::AnyPool;
use tokio::sync::{RwLock, mpsc};
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::Message as TungMessage;
use tracing::{error, info, warn};

use super::{
    client as s2s_client_mod,
    identity::{ServerIdentity, load_shared_secret},
    messaging,
    models::{FederatedPostEntry, S2SRequestMeta},
    repo,
    signing::{sign_request, verify_request},
};
use crate::config::Settings;
use crate::utils::encryption::ContentEncryption;
use magnolia_common::repositories::MediaRepository;
use uuid::Uuid;

// Public types

pub type HubRegistry = Arc<RwLock<HashMap<String, mpsc::UnboundedSender<HubOutbound>>>>;
pub type PeerStatusMap = Arc<RwLock<HashMap<String, PeerWsStatus>>>;

#[derive(Clone, Serialize)]
pub struct PeerWsStatus {
    pub conn_id: String,
    pub address: String,
    pub state: PeerState,
    pub offline_since: Option<String>,
}

#[derive(Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PeerState {
    Connected,
    Reconnecting,
    PeerOffline,
    Gone,
}

pub enum HubOutbound {
    Send(WsEvent),
    Shutdown,
    PeerGoingOffline,
}

// Wire format

#[derive(Serialize, Deserialize, Clone)]
pub struct WsHello {
    pub address: String,
    pub timestamp: i64,
    pub nonce: String,
    pub signature: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    Hello(WsHello),
    NewPost {
        entry: FederatedPostEntry,
    },
    /// Relay a call signaling payload (call_incoming, sdp_offer, ice_candidate, etc.)
    /// across the server-to-server WebSocket hub.
    CallSignal {
        payload: serde_json::Value,
    },
    GoingOffline,
    Ping,
    Pong,
}

// Global registries

static HUB_REGISTRY: OnceLock<HubRegistry> = OnceLock::new();
static PEER_STATUS: OnceLock<PeerStatusMap> = OnceLock::new();
static HUB_S2S_CLIENT: OnceLock<s2s_client_mod::S2SClient> = OnceLock::new();
static HUB_IDENTITY: OnceLock<Arc<ServerIdentity>> = OnceLock::new();
static HUB_SETTINGS: OnceLock<Arc<Settings>> = OnceLock::new();

/// Maps call_id → server_connection_id so that responses (call_accept etc.)
/// from a remote server can be routed back to the originating server.
static CALL_RELAY: OnceLock<Arc<RwLock<HashMap<String, String>>>> = OnceLock::new();

fn call_relay_map() -> &'static Arc<RwLock<HashMap<String, String>>> {
    CALL_RELAY.get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
}

pub fn new_registry() -> HubRegistry {
    Arc::new(RwLock::new(HashMap::new()))
}
pub fn new_status_map() -> PeerStatusMap {
    Arc::new(RwLock::new(HashMap::new()))
}

pub fn init_global(registry: HubRegistry, status: PeerStatusMap) {
    let _ = HUB_REGISTRY.set(registry);
    let _ = PEER_STATUS.set(status);
}

/// Deliver a call signal to a peer server via HTTP POST (encrypted S2S).
/// Fire-and-forget: logs on failure but does not propagate errors.
pub async fn send_call_signal_to_peer(pool: &AnyPool, conn_id: &str, payload: serde_json::Value) {
    let (client, identity, settings) =
        match (HUB_S2S_CLIENT.get(), HUB_IDENTITY.get(), HUB_SETTINGS.get()) {
            (Some(c), Some(i), Some(s)) => (c, i, s),
            _ => {
                warn!("send_call_signal_to_peer: hub globals not initialized");
                return;
            }
        };

    let conn = match repo::get_connection_by_id(pool, conn_id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            warn!("send_call_signal_to_peer: conn {} not found", conn_id);
            return;
        }
        Err(e) => {
            warn!("send_call_signal_to_peer: DB error: {}", e);
            return;
        }
    };

    let enc = settings
        .encryption_at_rest_key
        .as_deref()
        .and_then(|key| ContentEncryption::from_hex_key(key).ok());

    let raw_secret = match conn.shared_secret.as_deref() {
        Some(s) => s,
        None => {
            warn!(
                "send_call_signal_to_peer: no shared secret for conn {}",
                conn_id
            );
            return;
        }
    };

    let shared_secret = match load_shared_secret(raw_secret, enc.as_ref()) {
        Ok(s) => s,
        Err(e) => {
            warn!(
                "send_call_signal_to_peer: failed to load shared secret: {}",
                e
            );
            return;
        }
    };

    let our_address = settings.base_url.trim_end_matches('/');

    info!(
        "send_call_signal_to_peer: sending {:?} to {}",
        payload.get("type"),
        conn.address
    );
    if let Err(e) = s2s_client_mod::send_call_signal(
        client,
        identity,
        our_address,
        &conn.address,
        &payload,
        &shared_secret,
    )
    .await
    {
        warn!("send_call_signal_to_peer: {}", e);
    }
}

pub fn global_registry() -> Option<&'static HubRegistry> {
    HUB_REGISTRY.get()
}
pub fn global_status() -> Option<&'static PeerStatusMap> {
    PEER_STATUS.get()
}

/// Store a call_id → server_connection_id relay entry so responses to this
/// call can be forwarded to the originating server.
pub async fn register_federated_call(call_id: &str, server_connection_id: &str) {
    call_relay_map()
        .write()
        .await
        .insert(call_id.to_string(), server_connection_id.to_string());
}

/// Retrieve the server_connection_id that originated a federated call, if known.
pub async fn get_federated_call_origin(call_id: &str) -> Option<String> {
    call_relay_map().read().await.get(call_id).cloned()
}

/// Remove a relay entry once the call has ended.
pub async fn remove_federated_call(call_id: &str) {
    call_relay_map().write().await.remove(call_id);
}

/// Fire-and-forget: send a `CallSignal` payload to a specific peer connection
/// via the hub outbound channel.
pub fn send_call_signal_to_conn(conn_id: &str, payload: serde_json::Value) {
    if let Some(registry) = global_registry() {
        let conn_id = conn_id.to_string();
        let registry = registry.clone();
        tokio::spawn(async move {
            let guard = registry.read().await;
            if let Some(tx) = guard.get(&conn_id) {
                let _ = tx.send(HubOutbound::Send(WsEvent::CallSignal { payload }));
            }
        });
    }
}

// Startup

pub async fn start_hub(
    pool: AnyPool,
    settings: Arc<Settings>,
    identity: Arc<ServerIdentity>,
    registry: HubRegistry,
    status: PeerStatusMap,
) {
    // Make these available to send_call_signal_to_peer without passing through ws.rs.
    let _ = HUB_S2S_CLIENT.set(s2s_client_mod::build_client());
    let _ = HUB_IDENTITY.set(Arc::clone(&identity));
    let _ = HUB_SETTINGS.set(Arc::clone(&settings));

    let connections = match repo::list_active_connections(&pool).await {
        Ok(c) => c,
        Err(e) => {
            error!("hub::start_hub: {}", e);
            return;
        }
    };
    for conn in connections {
        spawn_outbound_for_peer(
            pool.clone(),
            Arc::clone(&settings),
            Arc::clone(&identity),
            registry.clone(),
            status.clone(),
            conn.id,
            conn.address,
        );
    }
}

pub fn spawn_outbound_for_peer(
    pool: AnyPool,
    settings: Arc<Settings>,
    identity: Arc<ServerIdentity>,
    registry: HubRegistry,
    status: PeerStatusMap,
    conn_id: String,
    peer_address: String,
) {
    let (tx, rx) = mpsc::unbounded_channel::<HubOutbound>();
    let reg_clone = registry.clone();
    let conn_id_reg = conn_id.clone();
    tokio::spawn(async move {
        let mut guard = reg_clone.write().await;
        if let Some(old_tx) = guard.remove(&conn_id_reg) {
            let _ = old_tx.send(HubOutbound::Shutdown);
        }
        guard.insert(conn_id_reg, tx);
    });
    tokio::spawn(outbound_task(
        pool,
        settings,
        identity,
        registry,
        status,
        conn_id,
        peer_address,
        rx,
    ));
}

pub async fn deregister_peer(registry: &HubRegistry, conn_id: &str) {
    let mut guard = registry.write().await;
    if let Some(tx) = guard.remove(conn_id) {
        let _ = tx.send(HubOutbound::Shutdown);
    }
}

// Broadcast

pub fn broadcast_new_post(entry: &FederatedPostEntry) {
    if let Some(registry) = HUB_REGISTRY.get() {
        let entry = entry.clone();
        let registry = registry.clone();
        tokio::spawn(async move {
            let guard = registry.read().await;
            for tx in guard.values() {
                let _ = tx.send(HubOutbound::Send(WsEvent::NewPost {
                    entry: entry.clone(),
                }));
            }
        });
    }
}

pub async fn broadcast_going_offline() {
    let Some(registry) = HUB_REGISTRY.get() else {
        return;
    };
    let guard = registry.read().await;
    for tx in guard.values() {
        let _ = tx.send(HubOutbound::Send(WsEvent::GoingOffline));
    }
    sleep(Duration::from_millis(500)).await;
}

pub async fn get_peer_statuses() -> Vec<PeerWsStatus> {
    let Some(map) = PEER_STATUS.get() else {
        return vec![];
    };
    map.read().await.values().cloned().collect()
}

// Outbound task

const PHASE1_END: Duration = Duration::from_secs(60);
const PHASE2_END: Duration = Duration::from_secs(11 * 60);
const PHASE3_END: Duration = Duration::from_secs(24 * 3600);

const RETRY_PHASE1: Duration = Duration::from_secs(10);
const RETRY_PHASE2: Duration = Duration::from_secs(60);
const RETRY_PHASE3: Duration = Duration::from_secs(15 * 60);

async fn outbound_task(
    pool: AnyPool,
    settings: Arc<Settings>,
    identity: Arc<ServerIdentity>,
    registry: HubRegistry,
    status: PeerStatusMap,
    conn_id: String,
    mut peer_address: String,
    mut rx: mpsc::UnboundedReceiver<HubOutbound>,
) {
    let our_address = settings.base_url.trim_end_matches('/').to_string();
    let mut ws_url = peer_to_ws_url(&peer_address);
    let mut offline_since: Option<Instant> = None;

    set_status(
        &status,
        &conn_id,
        &peer_address,
        PeerState::Reconnecting,
        offline_since,
    )
    .await;
    info!("hub: starting outbound task for {} ({})", conn_id, ws_url);

    loop {
        // Re-read the current address from DB on each attempt, the address may
        // have been updated since this task was spawned (e.g. peer IP/domain change).
        match repo::get_connection_by_id(&pool, &conn_id).await {
            Ok(Some(conn)) if conn.address != peer_address => {
                info!(
                    "hub: outbound address for {} updated: {} → {}",
                    conn_id, peer_address, conn.address
                );
                peer_address = conn.address.clone();
                ws_url = peer_to_ws_url(&peer_address);
                set_status(
                    &status,
                    &conn_id,
                    &peer_address,
                    PeerState::Reconnecting,
                    offline_since,
                )
                .await;
            }
            Ok(None) => {
                // Connection was deleted, probably intentionally, stop this task.
                info!(
                    "hub: connection {} no longer exists, stopping outbound task",
                    conn_id
                );
                registry.write().await.remove(&conn_id);
                status.write().await.remove(&conn_id);
                return;
            }
            _ => {}
        }

        match try_connect_and_run(
            &ws_url,
            &our_address,
            &identity,
            &pool,
            &conn_id,
            &peer_address,
            &mut rx,
        )
        .await
        {
            OutboundResult::Shutdown => {
                info!("hub: outbound task for {} shutting down", conn_id);
                registry.write().await.remove(&conn_id);
                status.write().await.remove(&conn_id);
                return;
            }
            OutboundResult::PeerGoingOffline => {
                info!(
                    "hub: {} sent GoingOffline — waiting for peer to re-initiate",
                    peer_address
                );
                registry.write().await.remove(&conn_id);
                set_status(
                    &status,
                    &conn_id,
                    &peer_address,
                    PeerState::PeerOffline,
                    None,
                )
                .await;
                return;
            }
            OutboundResult::WasConnected => {
                offline_since = None;
                set_status(
                    &status,
                    &conn_id,
                    &peer_address,
                    PeerState::Reconnecting,
                    None,
                )
                .await;
                sleep(RETRY_PHASE1).await;
            }
            OutboundResult::ConnectFailed(ref e) => {
                let start = offline_since.get_or_insert(Instant::now());
                let elapsed = start.elapsed();

                if elapsed >= PHASE3_END {
                    warn!("hub: {} unreachable >=24 h — marking Gone", peer_address);
                    registry.write().await.remove(&conn_id);
                    set_status(
                        &status,
                        &conn_id,
                        &peer_address,
                        PeerState::Gone,
                        offline_since,
                    )
                    .await;
                    return;
                }

                let delay = if elapsed < PHASE1_END {
                    RETRY_PHASE1
                } else if elapsed < PHASE2_END {
                    RETRY_PHASE2
                } else {
                    RETRY_PHASE3
                };

                warn!("hub: {} failed: {}; retry in {:?}", peer_address, e, delay);
                set_status(
                    &status,
                    &conn_id,
                    &peer_address,
                    PeerState::Reconnecting,
                    offline_since,
                )
                .await;
                sleep(delay).await;
            }
        }
    }
}

enum OutboundResult {
    Shutdown,
    PeerGoingOffline,
    WasConnected,
    ConnectFailed(String),
}

async fn try_connect_and_run(
    ws_url: &str,
    our_address: &str,
    identity: &ServerIdentity,
    pool: &AnyPool,
    conn_id: &str,
    peer_address: &str,
    rx: &mut mpsc::UnboundedReceiver<HubOutbound>,
) -> OutboundResult {
    let (ws_stream, _) = match tokio_tungstenite::connect_async(ws_url).await {
        Ok(v) => v,
        Err(e) => return OutboundResult::ConnectFailed(e.to_string()),
    };
    let (mut sink, mut stream) = ws_stream.split();

    let hello = build_hello(our_address, identity);
    let hello_json = match serde_json::to_string(&WsEvent::Hello(hello)) {
        Ok(j) => j,
        Err(e) => return OutboundResult::ConnectFailed(e.to_string()),
    };
    if let Err(e) = sink.send(TungMessage::Text(hello_json.into())).await {
        return OutboundResult::ConnectFailed(e.to_string());
    }
    info!("hub: outbound WS connected to {}", peer_address);

    // Drain any messages queued while this peer was unreachable.
    if let (Some(hub_settings), Some(hub_identity)) = (HUB_SETTINGS.get(), HUB_IDENTITY.get()) {
        tokio::spawn(messaging::drain_pending_for_peer(
            pool.clone(),
            Arc::clone(hub_settings),
            Arc::clone(hub_identity),
            HUB_S2S_CLIENT
                .get()
                .cloned()
                .unwrap_or_else(s2s_client_mod::build_client),
            conn_id.to_string(),
            peer_address.to_string(),
        ));
    }

    loop {
        tokio::select! {
        cmd = rx.recv() => match cmd {
        None | Some(HubOutbound::Shutdown) => return OutboundResult::Shutdown,
        Some(HubOutbound::PeerGoingOffline) => return OutboundResult::PeerGoingOffline,
        Some(HubOutbound::Send(event)) => {
        if let Ok(json) = serde_json::to_string(&event) {
        if sink.send(TungMessage::Text(json.into())).await.is_err() {
        return OutboundResult::WasConnected;
        }
        }
        }
        },
        inbound = stream.next() => match inbound {
        None | Some(Err(_)) => return OutboundResult::WasConnected,
        Some(Ok(TungMessage::Close(_))) => return OutboundResult::WasConnected,
        Some(Ok(TungMessage::Ping(data))) => { let _ = sink.send(TungMessage::Pong(data)).await; }
        Some(Ok(TungMessage::Text(text))) => {
        if matches!(serde_json::from_str::<WsEvent>(text.as_str()), Ok(WsEvent::GoingOffline)) {
        info!("hub: {} sent going_offline on outbound channel", peer_address);
        return OutboundResult::PeerGoingOffline;
        }
        handle_inbound_event(pool, conn_id, peer_address, text.as_str()).await;
        }
        _ => {}
        },
        }
    }
}

// Inbound WS handler

pub async fn handle_peer_ws(
    socket: WebSocket,
    pool: AnyPool,
    settings: Arc<Settings>,
    identity: Arc<ServerIdentity>,
    registry: HubRegistry,
    status: PeerStatusMap,
) {
    let (mut sink, mut stream) = socket.split();

    let hello_text = match tokio::time::timeout(Duration::from_secs(30), stream.next()).await {
        Ok(Some(Ok(AxMessage::Text(t)))) => t,
        _ => {
            warn!("hub: inbound WS: no Hello within timeout");
            return;
        }
    };

    let hello = match serde_json::from_str::<WsEvent>(&hello_text) {
        Ok(WsEvent::Hello(h)) => h,
        _ => {
            warn!("hub: inbound WS: first message is not a Hello");
            return;
        }
    };

    let hello_address = hello.address.trim_end_matches('/').to_string();

    let sig_bytes = match hex::decode(&hello.signature) {
        Ok(b) => b,
        Err(_) => {
            warn!(
                "hub: inbound WS hello from {} has invalid signature encoding",
                hello_address
            );
            return;
        }
    };
    let meta = S2SRequestMeta {
        sender_address: hello_address.clone(),
        timestamp: hello.timestamp,
        nonce: hello.nonce.clone(),
        signature: sig_bytes,
    };

    // Helper: verify the hello signature against a connection's stored key.
    let verify_conn = |conn: &super::models::ServerConnectionRow| -> bool {
        let vk_bytes = match conn.their_ml_dsa_public_key.as_deref() {
            Some(b) => b,
            None => return false,
        };
        let vk = match super::signing::decode_verifying_key(vk_bytes) {
            Ok(k) => k,
            Err(_) => return false,
        };
        verify_request(hello_address.as_bytes(), &meta, &vk).is_ok()
    };

    // The accepting side may spawn its outbound WS before the initiating side
    // has finished processing the HTTP accept response and activating the record.
    // Retry for up to 10 s to let the activation settle.
    let conn = {
        let mut found = None;
        for attempt in 0..10u8 {
            match repo::get_connection_by_address(&pool, &hello_address).await {
                Ok(Some(c)) if c.status == "active" => {
                    found = Some(c);
                    break;
                }
                Ok(Some(c)) if c.status == "pending_out" || c.status == "pending_in" => {
                    if attempt == 0 {
                        info!(
                            "hub: inbound WS from {} — connection still pending ({}), waiting for activation",
                            hello_address, c.status
                        );
                    }
                    sleep(Duration::from_secs(1)).await;
                }
                Ok(Some(c)) => {
                    warn!(
                        "hub: inbound WS hello from {} rejected — connection status is '{}'",
                        hello_address, c.status
                    );
                    return;
                }
                Ok(None) => {
                    // Address lookup failed, the peer may be calling from a different
                    // address (VPN, IP change). Scan all active connections by key.
                    match repo::list_active_connections(&pool).await {
                        Ok(all) => {
                            if let Some(c) = all.into_iter().find(|c| verify_conn(c)) {
                                info!(
                                    "hub: inbound WS from {} matched by key to conn {} (was '{}')",
                                    hello_address, c.id, c.address
                                );
                                // Update the stored address so future fast-path lookups work.
                                if c.address != hello_address {
                                    let _ = repo::update_connection_address(
                                        &pool,
                                        &c.id,
                                        &hello_address,
                                    )
                                    .await;
                                }
                                found = Some(c);
                                break;
                            } else {
                                warn!(
                                    "hub: inbound WS hello from {} rejected — no matching connection found",
                                    hello_address
                                );
                                return;
                            }
                        }
                        Err(e) => {
                            warn!(
                                "hub: DB error scanning connections for {}: {}",
                                hello_address, e
                            );
                            return;
                        }
                    }
                }
                Err(e) => {
                    warn!("hub: DB error looking up peer {}: {}", hello_address, e);
                    return;
                }
            }
        }
        match found {
            Some(c) => c,
            None => {
                warn!(
                    "hub: inbound WS from {} — still pending after 10s wait, giving up",
                    hello_address
                );
                return;
            }
        }
    };

    if !verify_conn(&conn) {
        warn!(
            "hub: inbound WS hello from {} has invalid signature",
            hello_address
        );
        return;
    }

    info!(
        "hub: inbound WS authenticated from {} (conn {})",
        hello_address, conn.id
    );

    // If the peer was Gone/PeerOffline, re-spawn our outbound task.
    // Use hello_address (current address) not conn.address (potentially stale).
    if !registry.read().await.contains_key(&conn.id) {
        info!(
            "hub: peer {} reconnected inbound — spawning outbound task",
            hello_address
        );
        spawn_outbound_for_peer(
            pool.clone(),
            Arc::clone(&settings),
            Arc::clone(&identity),
            registry.clone(),
            status.clone(),
            conn.id.clone(),
            hello_address.clone(),
        );

        // Also drain pending messages, the outbound task may not fire immediately. Want to avoid double send.
        tokio::spawn(messaging::drain_pending_for_peer(
            pool.clone(),
            Arc::clone(&settings),
            Arc::clone(&identity),
            HUB_S2S_CLIENT
                .get()
                .cloned()
                .unwrap_or_else(s2s_client_mod::build_client),
            conn.id.clone(),
            hello_address.clone(),
        ));
    }

    while let Some(msg) = stream.next().await {
        match msg {
            Ok(AxMessage::Text(text)) => {
                handle_inbound_event(&pool, &conn.id, &hello_address, &text).await
            }
            Ok(AxMessage::Ping(data)) => {
                let _ = sink.send(AxMessage::Pong(data)).await;
            }
            Ok(AxMessage::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
    info!("hub: inbound WS from {} closed", hello_address);
}

// Inbound event dispatch

/// Create local media stubs for any `media_ref` items in a federated post's contents,
/// rewriting `media_ref.media_id` and `content` to local IDs so the feed handler can
/// serve them without knowing the originating server's media IDs.
async fn create_post_media_stubs(
    pool: &AnyPool,
    entry: &mut FederatedPostEntry,
    peer_address: &str,
) {
    let media_repo = MediaRepository::new(pool.clone());
    let now_str = Utc::now().to_rfc3339();
    for c in &mut entry.contents {
        if let Some(ref mut att) = c.media_ref {
            let local_id = match media_repo.find_by_origin(peer_address, &att.media_id).await {
                Ok(Some(existing)) => existing.media_id,
                _ => {
                    let new_id = Uuid::new_v4().to_string();
                    if let Err(e) = media_repo
                        .create_federated_stub(
                            &new_id,
                            "__fed__",
                            &att.media_type,
                            &att.filename,
                            &att.mime_type,
                            att.file_size,
                            att.width,
                            att.height,
                            peer_address,
                            &att.media_id,
                            &now_str,
                        )
                        .await
                    {
                        warn!("hub: create_post_media_stubs: stub failed: {}", e);
                        continue;
                    }
                    new_id
                }
            };
            att.media_id = local_id.clone();
            c.content = local_id.clone();
        }
    }
}

async fn handle_inbound_event(pool: &AnyPool, conn_id: &str, peer_address: &str, text: &str) {
    let event: WsEvent = match serde_json::from_str(text) {
        Ok(e) => e,
        Err(_) => return,
    };
    match event {
        WsEvent::NewPost { mut entry } => {
            create_post_media_stubs(pool, &mut entry, peer_address).await;
            let content_hash = compute_content_hash(&entry);
            let content_json = match serde_json::to_string(&entry) {
                Ok(j) => j,
                Err(_) => return,
            };
            if let Err(e) = repo::upsert_post_ref(
                pool,
                conn_id,
                &entry.user_id,
                &entry.post_id,
                &entry.posted_at,
                &content_hash,
                &content_json,
                &entry.server_address,
            )
            .await
            {
                error!("hub: upsert_post_ref failed: {}", e);
            }
        }
        WsEvent::CallSignal { payload } => {
            // Spawn to avoid growing the inbound WS loop's future chain.
            let pool_clone = pool.clone();
            let conn_id_owned = conn_id.to_string();
            tokio::spawn(async move {
                handle_inbound_call_signal(&pool_clone, &conn_id_owned, payload).await;
            });
        }
        WsEvent::GoingOffline => info!("hub: {} notified going offline (inbound)", peer_address),
        _ => {}
    }
}

/// Route an inbound call signal from a peer server to local users or back-relay to origin.
pub async fn handle_inbound_call_signal(pool: &AnyPool, conn_id: &str, payload: serde_json::Value) {
    let type_str = match payload.get("type").and_then(|v| v.as_str()) {
        Some(t) => t.to_string(),
        None => return,
    };

    match type_str.as_str() {
        // A remote server is notifying a specific local user of an incoming call.
        "call_incoming" | "open_call_available" => {
            // Register so response signals (call_accept etc.) can be relayed back.
            if let Some(call_id) = payload.get("call_id").and_then(|v| v.as_str()) {
                register_federated_call(call_id, conn_id).await;
            }
            // Route directly to the named recipient, bypassing conversation ID lookup
            // (the conversation_id in the payload is the *sender* server's local UUID,
            // which doesn't exist in this server's database).
            let msg = payload.to_string();
            if let Some(uid) = payload.get("recipient_user_id").and_then(|v| v.as_str()) {
                if let Some(reg) = crate::handlers::ws::global_user_registry() {
                    crate::handlers::ws::send_to_user(reg, uid, &msg).await;
                }
            } else if let Some(conv_id) = payload.get("conversation_id").and_then(|v| v.as_str()) {
                // Fallback: resolve via local conversation members (same-server group calls).
                route_call_signal_to_local_members(pool, conv_id, &payload).await;
            }
        }
        "call_ended" => {
            if let Some(call_id) = payload.get("call_id").and_then(|v| v.as_str()) {
                remove_federated_call(call_id).await;
            }
            // call_ended may come back from the remote server; route to recipient if named.
            let msg = payload.to_string();
            if let Some(uid) = payload.get("recipient_user_id").and_then(|v| v.as_str()) {
                if let Some(reg) = crate::handlers::ws::global_user_registry() {
                    crate::handlers::ws::send_to_user(reg, uid, &msg).await;
                }
            } else if let Some(conv_id) = payload.get("conversation_id").and_then(|v| v.as_str()) {
                route_call_signal_to_local_members(pool, conv_id, &payload).await;
            }
        }
        // All other signals (call_accept relay, call_reject, sdp_offer, ice_candidate, …)
        // are handed off to the WS handler which has the call repository and user registry.
        _ => {
            crate::handlers::ws::relay_federated_call_signal(pool, conn_id, payload).await;
        }
    }
}

/// Send a call signal JSON payload to every local (non-federated) member of a conversation.
async fn route_call_signal_to_local_members(
    pool: &AnyPool,
    conv_id: &str,
    payload: &serde_json::Value,
) {
    use sqlx::Row;
    let msg = payload.to_string();
    let rows =
        match sqlx::query("SELECT user_id FROM conversation_members WHERE conversation_id = $1")
            .bind(conv_id)
            .fetch_all(pool)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                error!("hub: route_call_signal_to_local_members DB error: {}", e);
                return;
            }
        };

    if let Some(reg) = crate::handlers::ws::global_user_registry() {
        for row in rows {
            let uid: String = row.get("user_id");
            if uid == "__fed__" {
                continue;
            }
            crate::handlers::ws::send_to_user(reg, &uid, &msg).await;
        }
    }
}

// Helpers

async fn set_status(
    status: &PeerStatusMap,
    conn_id: &str,
    address: &str,
    state: PeerState,
    offline_since: Option<Instant>,
) {
    let offline_since_str = offline_since.map(|start| {
        let secs = start.elapsed().as_secs();
        (Utc::now() - chrono::Duration::seconds(secs as i64)).to_rfc3339()
    });
    status.write().await.insert(
        conn_id.to_string(),
        PeerWsStatus {
            conn_id: conn_id.to_string(),
            address: address.to_string(),
            state,
            offline_since: offline_since_str,
        },
    );
}

fn compute_content_hash(entry: &FederatedPostEntry) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_string(&entry.contents)
            .unwrap_or_default()
            .as_bytes(),
    ))
}

fn peer_to_ws_url(peer_address: &str) -> String {
    let base = peer_address.trim_end_matches('/');
    if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{}/api/s2s/ws", rest)
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{}/api/s2s/ws", rest)
    } else {
        format!("ws://{}/api/s2s/ws", base)
    }
}

fn build_hello(our_address: &str, identity: &ServerIdentity) -> WsHello {
    let signing_key = identity.signing_key();
    let (timestamp, nonce, sig) = sign_request(our_address.as_bytes(), &signing_key);
    WsHello {
        address: our_address.to_string(),
        timestamp,
        nonce,
        signature: sig,
    }
}
