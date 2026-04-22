CREATE TABLE IF NOT EXISTS proxy_sessions (
    session_id TEXT PRIMARY KEY,
    proxy_id TEXT NOT NULL REFERENCES proxy_accounts(proxy_id) ON DELETE CASCADE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);
