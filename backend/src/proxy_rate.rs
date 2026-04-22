//! In-memory rate limiting for proxy HMAC media uploads.
//!
//! Tracks per-proxy upload counts and byte totals within rolling 1-minute windows.
//! State resets on server restart (intentional, this is a soft guard, not audit log).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug)]
struct RateWindow {
    /// Start of the current 1-minute window
    window_start: Instant,
    /// Number of uploads in the current window
    pieces: u32,
    /// Total bytes uploaded in the current window
    bytes: u64,
}

impl RateWindow {
    fn new() -> Self {
        Self {
            window_start: Instant::now(),
            pieces: 0,
            bytes: 0,
        }
    }

    /// Reset if the window has expired, then record the upload.
    /// Returns `(pieces_after, bytes_after)` in the current window.
    fn record(&mut self, file_bytes: u64) -> (u32, u64) {
        if self.window_start.elapsed() >= Duration::from_secs(60) {
            self.window_start = Instant::now();
            self.pieces = 0;
            self.bytes = 0;
        }
        self.pieces += 1;
        self.bytes += file_bytes;
        (self.pieces, self.bytes)
    }
}

/// Shared rate-limit state, held in `AppState`.
#[derive(Clone, Debug)]
pub struct ProxyRateLimiter {
    inner: Arc<Mutex<HashMap<String, RateWindow>>>,
}

impl ProxyRateLimiter {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Record an upload and check whether the proxy has exceeded either limit.
    ///
    /// Returns `Ok(())` if within limits, `Err(())` if the limit was breached.
    /// The upload is counted even on breach so the caller can disable the proxy.
    pub fn check_and_record(
        &self,
        proxy_id: &str,
        file_bytes: u64,
        max_pieces: u32,
        max_bytes: u64,
    ) -> Result<(), ()> {
        let mut map = self.inner.lock().unwrap();
        let window = map
            .entry(proxy_id.to_string())
            .or_insert_with(RateWindow::new);
        let (pieces, bytes) = window.record(file_bytes);
        if pieces > max_pieces || bytes > max_bytes {
            Err(())
        } else {
            Ok(())
        }
    }

    /// Remove tracking state for a proxy (e.g. after disabling it).
    pub fn clear(&self, proxy_id: &str) {
        self.inner.lock().unwrap().remove(proxy_id);
    }
}

impl Default for ProxyRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}
