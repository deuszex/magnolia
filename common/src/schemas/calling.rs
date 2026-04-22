use serde::{Deserialize, Serialize};
use validator::Validate;

/// Admin response for a single STUN/TURN server entry.
/// Credential is intentionally omitted, so that the frontend doen't leak credentials. Write-only once set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StunServerResponse {
    pub id: String,
    pub url: String,
    pub username: Option<String>,
    pub has_credential: bool,
    pub enabled: bool,
    pub last_checked_at: Option<String>,
    pub last_status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateStunServerRequest {
    #[validate(length(min = 1, max = 512))]
    pub url: String,
    #[validate(length(max = 256))]
    pub username: Option<String>,
    #[validate(length(max = 256))]
    pub credential: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateStunServerRequest {
    #[validate(length(min = 1, max = 512))]
    pub url: Option<String>,
    #[validate(length(max = 256))]
    pub username: Option<String>,
    /// Pass null to clear, omit to leave unchanged, pass a value to update.
    pub credential: Option<String>,
    pub enabled: Option<bool>,
}

fn default_true() -> bool {
    true
}

/// Request to initiate a call
#[derive(Debug, Serialize, Deserialize)]
pub struct InitiateCallRequest {
    pub call_type: String,
    /// If true, any conversation member can join without an invitation.
    #[serde(default)]
    pub open: bool,
}

/// Response for a call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallResponse {
    pub call_id: String,
    pub conversation_id: String,
    pub call_type: String,
    pub status: String,
    pub initiated_by: String,
    pub participants: Vec<CallParticipantResponse>,
    pub created_at: String,
}

/// Participant info in a call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallParticipantResponse {
    pub user_id: String,
    pub display_name: Option<String>,
    pub role: String,
    pub status: String,
}

/// ICE server configuration for WebRTC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceConfigResponse {
    pub ice_servers: Vec<IceServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

/// Call history list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallHistoryResponse {
    pub calls: Vec<CallHistoryEntry>,
    pub has_more: bool,
}

/// Single call history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallHistoryEntry {
    pub call_id: String,
    pub conversation_id: String,
    pub conversation_name: Option<String>,
    pub call_type: String,
    pub status: String,
    pub initiated_by: String,
    pub initiator_name: Option<String>,
    pub participants: Vec<CallParticipantResponse>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub duration_seconds: Option<i32>,
    pub created_at: String,
}

/// Query parameters for call history
#[derive(Debug, Deserialize)]
pub struct CallHistoryQuery {
    #[serde(default = "default_call_limit")]
    pub limit: i32,
    #[serde(default)]
    pub offset: i32,
}

fn default_call_limit() -> i32 {
    50
}

/// Single participant in the global call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalCallParticipantResponse {
    pub user_id: String,
    pub display_name: Option<String>,
    pub joined_at: String,
}

/// Response for GET /api/global-call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalCallResponse {
    pub participants: Vec<GlobalCallParticipantResponse>,
}

/// WebSocket signaling messages (client -> server)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SignalMessage {
    #[serde(rename = "call_initiate")]
    CallInitiate {
        conversation_id: String,
        call_type: String,
        /// If true, any conversation member can join without an invitation.
        #[serde(default)]
        open: bool,
    },
    #[serde(rename = "call_accept")]
    CallAccept { call_id: String },
    #[serde(rename = "call_reject")]
    CallReject { call_id: String },
    #[serde(rename = "call_hangup")]
    CallHangup { call_id: String },
    #[serde(rename = "call_busy")]
    CallBusy { call_id: String },
    #[serde(rename = "call_join")]
    CallJoin { call_id: String },
    #[serde(rename = "ice_candidate")]
    IceCandidate {
        call_id: String,
        target_user_id: String,
        candidate: serde_json::Value,
    },
    #[serde(rename = "sdp_offer")]
    SdpOffer {
        call_id: String,
        target_user_id: String,
        sdp: String,
    },
    #[serde(rename = "sdp_answer")]
    SdpAnswer {
        call_id: String,
        target_user_id: String,
        sdp: String,
    },
    #[serde(rename = "key_exchange")]
    KeyExchange {
        call_id: String,
        target_user_id: String,
        public_key: String, // base64-encoded 32-byte X25519 public key
    },
    /// Initiator forcibly removes a participant from the call.
    #[serde(rename = "call_kick")]
    CallKick {
        call_id: String,
        target_user_id: String,
    },
    /// Global call: forward offer to a specific participant
    #[serde(rename = "global_call_offer")]
    GlobalCallOffer { target_user_id: String, sdp: String },
    /// Global call: forward answer to a specific participant
    #[serde(rename = "global_call_answer")]
    GlobalCallAnswer { target_user_id: String, sdp: String },
    /// Global call: forward ICE candidate to a specific participant
    #[serde(rename = "global_call_ice")]
    GlobalCallIce {
        target_user_id: String,
        candidate: serde_json::Value,
    },
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
}

impl SignalMessage {
    pub fn type_name(&self) -> &'static str {
        match self {
            SignalMessage::CallInitiate { .. } => "call_initiate",
            SignalMessage::CallAccept { .. } => "call_accept",
            SignalMessage::CallReject { .. } => "call_reject",
            SignalMessage::CallHangup { .. } => "call_hangup",
            SignalMessage::CallBusy { .. } => "call_busy",
            SignalMessage::CallJoin { .. } => "call_join",
            SignalMessage::IceCandidate { .. } => "ice_candidate",
            SignalMessage::SdpOffer { .. } => "sdp_offer",
            SignalMessage::SdpAnswer { .. } => "sdp_answer",
            SignalMessage::KeyExchange { .. } => "key_exchange",
            SignalMessage::CallKick { .. } => "call_kick",
            SignalMessage::GlobalCallOffer { .. } => "global_call_offer",
            SignalMessage::GlobalCallAnswer { .. } => "global_call_answer",
            SignalMessage::GlobalCallIce { .. } => "global_call_ice",
            SignalMessage::Ping => "ping",
            SignalMessage::Pong => "pong",
        }
    }
}
