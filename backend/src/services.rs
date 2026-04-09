use crate::config::Settings;
use magnolia_common::repositories::StunServerRepository;
use sqlx::AnyPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;

/// Email scheduler service (stub)
pub struct EmailSchedulerService {
    _pool: AnyPool,
    _settings: Arc<Settings>,
}

impl EmailSchedulerService {
    pub async fn new(
        pool: AnyPool,
        settings: Arc<Settings>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            _pool: pool,
            _settings: settings,
        })
    }

    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // TODO: Implement email scheduling
        Ok(())
    }
}

/// Periodically probes each admin-configured STUN server and updates last_status.
pub struct StunHealthCheckService {
    pool: AnyPool,
}

impl StunHealthCheckService {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    pub fn start(self) {
        tokio::spawn(async move {
            // Stagger startup so the server is fully initialised before the first probe.
            tokio::time::sleep(Duration::from_secs(30)).await;
            loop {
                self.run_checks().await;
                // Re-check every 5 minutes.
                tokio::time::sleep(Duration::from_secs(300)).await;
            }
        });
    }

    async fn run_checks(&self) {
        let repo = StunServerRepository::new(self.pool.clone());
        let servers = match repo.list_all().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("STUN health-check: failed to list servers: {e:?}");
                return;
            }
        };

        for server in servers {
            let status = probe_stun(&server.url).await;
            let now = chrono::Utc::now().to_rfc3339();
            if let Err(e) = repo.update_health(&server.id, status, &now).await {
                tracing::warn!("STUN health-check: failed to persist status for {}: {e:?}", server.url);
            } else {
                tracing::debug!("STUN health-check: {} → {}", server.url, status);
            }
        }
    }
}

/// Probe a STUN/TURN URL by sending a minimal RFC-5389 Binding Request over UDP
/// and waiting up to 3 seconds for any response from the server.
/// Returns "ok" or "unreachable".
async fn probe_stun(url: &str) -> &'static str {
    // Parse stun:host:port or stun:host (default port 3478).
    // Also accepts turn: prefix — we only test UDP reachability, not auth.
    let addr_part = url
        .trim_start_matches("stun:")
        .trim_start_matches("turn:")
        .split('?')
        .next()
        .unwrap_or("");

    let host_port = if addr_part.contains(':') {
        addr_part.to_string()
    } else {
        format!("{}:3478", addr_part)
    };

    let socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(_) => return "unreachable",
    };

    if socket.connect(&host_port).await.is_err() {
        return "unreachable";
    }

    // Minimal STUN Binding Request (20 bytes): type=0x0001, length=0, magic=0x2112A442, tx-id
    let mut req = [0u8; 20];
    req[0] = 0x00; req[1] = 0x01; // Message Type: Binding Request
    req[2] = 0x00; req[3] = 0x00; // Message Length: 0
    req[4] = 0x21; req[5] = 0x12; req[6] = 0xA4; req[7] = 0x42; // Magic Cookie
    // Transaction ID (12 bytes) — arbitrary
    req[8..20].copy_from_slice(b"magnolia_chk");

    if socket.send(&req).await.is_err() {
        return "unreachable";
    }

    let mut buf = [0u8; 512];
    match tokio::time::timeout(Duration::from_secs(3), socket.recv(&mut buf)).await {
        Ok(Ok(_)) => "ok",
        _ => "unreachable",
    }
}
