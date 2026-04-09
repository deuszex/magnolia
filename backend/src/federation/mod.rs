//! Server-to-server federation.
//!
//! Sub-modules:
//! - `identity` — server's own ML-DSA keypair (generate, persist, load)
//! - `models` — DB row structs and public-facing API types
//! - `repo` — all SQL queries for federation tables
//! - `client` — signed outbound HTTP requests to peer servers
//! - `handshake` — ML-KEM connection initiation / accept / reject logic
//! - `signing` — request signing and verification helpers (used by both client + handlers)
//! - `handlers` — Axum handler functions (admin + S2S endpoints)
//! - `routes` — route table assembly for inclusion in routes.rs
//! - `hub` — persistent WebSocket connections to active peers
//! - `relay` — discovery propagation logic

pub mod client;
pub mod handlers;
pub mod handshake;
pub mod hub;
pub mod identity;
pub mod messaging;
pub mod models;
pub mod posts_sync;
pub mod relay;
pub mod repo;
pub mod routes;
pub mod signing;
pub mod sync;
