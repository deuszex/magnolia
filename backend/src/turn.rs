use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::env;
use std::net::SocketAddr;

/// TURN server configuration
#[derive(Debug, Clone)]
pub struct TurnConfig {
    pub enabled: bool,
    pub listen_addr: SocketAddr,
    pub realm: String,
    pub auth_secret: String,
    pub external_ip: String,
}

impl TurnConfig {
    pub fn from_env() -> Result<Self, String> {
        let enabled =
            env::var("TURN_ENABLED").unwrap_or_else(|_| "false".to_string()) == "true";

        if !enabled {
            return Ok(TurnConfig {
                enabled: false,
                listen_addr: SocketAddr::from(([0, 0, 0, 0], 3478)),
                realm: String::new(),
                auth_secret: String::new(),
                external_ip: String::new(),
            });
        }

        let auth_secret = env::var("SESSION_SECRET")
            .ok()
            .filter(|s| !s.is_empty())
            .ok_or("TURN_ENABLED=true but SESSION_SECRET is not set")?;

        let external_ip = env::var("TURN_EXTERNAL_IP")
            .ok()
            .filter(|s| !s.is_empty())
            .ok_or("TURN_ENABLED=true but TURN_EXTERNAL_IP is not set")?;

        let listen_addr = env::var("TURN_LISTEN_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:3478".to_string())
            .parse()
            .map_err(|_| "Invalid TURN_LISTEN_ADDR — expected host:port".to_string())?;

        Ok(TurnConfig {
            enabled: true,
            listen_addr,
            realm: env::var("TURN_REALM").unwrap_or_else(|_| "magnolia".to_string()),
            auth_secret,
            external_ip,
        })
    }
}

/// Generate time-limited TURN credentials using HMAC-SHA256.
///
/// Uses the "temporary TURN credentials" pattern:
/// username = "{expiry_timestamp}:{user_id}"
/// password = Base64(HMAC-SHA256(secret, username))
///
/// These credentials are verified by the TURN server using the same shared secret.
pub fn generate_turn_credentials(
    user_id: &str,
    secret: &str,
    ttl_seconds: u64,
) -> (String, String) {
    let expiry = chrono::Utc::now().timestamp() as u64 + ttl_seconds;
    let username = format!("{}:{}", expiry, user_id);

    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(username.as_bytes());
    let password = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    (username, password)
}

/// Start the embedded TURN server as a background Tokio task.
///
/// Currently a placeholder — the actual TURN server implementation depends on
/// selecting and integrating a TURN crate (e.g. `turn-rs` SDK or `webrtc-turn`).
/// When TURN_ENABLED=false (default), this is a no-op.
pub async fn start_turn_server(config: &TurnConfig) {
    if !config.enabled {
        tracing::info!("Embedded TURN server disabled (set TURN_ENABLED=true to enable)");
        return;
    }

    tracing::info!(
        "TURN server configured: listen={}, realm={}, external_ip={}",
        config.listen_addr,
        config.realm,
        config.external_ip
    );

    // TODO: Integrate actual TURN server crate here.
    // The TURN server should:
    // 1. Bind to config.listen_addr (UDP + TCP)
    // 2. Use config.realm as the TURN realm
    // 3. Authenticate using the same HMAC-SHA256 credential scheme as generate_turn_credentials()
    // 4. Relay media traffic between peers that can't establish direct P2P connections
    //
    // Candidate crates:
    // - turn-rs v4 SDK (high-performance, pure Rust)
    // - webrtc-turn (from webrtc-rs monorepo)
    //
    // For now, STUN-only mode works for most LAN and non-restrictive NAT scenarios.
    // Set TURN_ENABLED=true and configure a TURN crate when relay support is needed.

    tracing::warn!("Embedded TURN server not yet implemented — using STUN-only mode");
}
