CREATE TABLE IF NOT EXISTS proxy_rate_limits (
    proxy_id TEXT PRIMARY KEY REFERENCES proxy_accounts(proxy_id) ON DELETE CASCADE,
    -- NULL means fall back to the server default from site_config
    max_pieces_per_minute INTEGER,
    max_bytes_per_minute INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
