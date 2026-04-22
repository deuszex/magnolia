CREATE TABLE IF NOT EXISTS site_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    media_storage_path TEXT NOT NULL DEFAULT './media_storage',
    allow_text_posts INTEGER NOT NULL DEFAULT 1,
    allow_image_posts INTEGER NOT NULL DEFAULT 1,
    allow_video_posts INTEGER NOT NULL DEFAULT 1,
    allow_file_posts INTEGER NOT NULL DEFAULT 1,
    encryption_at_rest_enabled INTEGER NOT NULL DEFAULT 0,
    message_auto_delete_enabled INTEGER NOT NULL DEFAULT 0,
    message_auto_delete_delay_hours INTEGER NOT NULL DEFAULT 168,
    registration_mode TEXT NOT NULL DEFAULT 'open',
    application_timeout_hours INTEGER NOT NULL DEFAULT 48,
    enforce_invite_email INTEGER NOT NULL DEFAULT 0,
    /*Whether federation is enabled at all on this server.*/
    federation_enabled INTEGER NOT NULL DEFAULT 0,
    /*Whether this server accepts incoming connection requests from strangers.*/
    federation_accept_incoming INTEGER NOT NULL DEFAULT 0,
    /*Whether this server announces its known connections to newly-connected peers.*/
    federation_share_connections INTEGER NOT NULL DEFAULT 0,
    /*Maximum persistent WebSocket connections to maintain toward peer servers.*/
    federation_max_connections INTEGER NOT NULL DEFAULT 50,
    /*How many relay hops of discovery this server will propagate (1 = direct peers only).*/
    federation_relay_depth INTEGER NOT NULL DEFAULT 1,
    password_reset_email_enabled INTEGER NOT NULL DEFAULT 1,
    password_reset_signing_key_enabled INTEGER NOT NULL DEFAULT 0,
    proxy_user_system INTEGER NOT NULL DEFAULT 0,
    proxy_rate_limit_pieces INTEGER NOT NULL DEFAULT 1,
    proxy_rate_limit_bytes INTEGER NOT NULL DEFAULT 12582912,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT OR IGNORE INTO site_config (id, created_at, updated_at) VALUES (1, datetime('now'), datetime('now'));