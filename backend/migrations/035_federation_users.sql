-- Remote users we cache minimal profiles for 
-- Only users who have opted in to sharing on the remote server are listed here.
-- No post content, no message content — just enough to identify them in the UI.
CREATE TABLE IF NOT EXISTS federation_users (
    id TEXT PRIMARY KEY, -- local UUID
    server_connection_id TEXT NOT NULL REFERENCES server_connections(id) ON DELETE CASCADE,
    remote_user_id TEXT NOT NULL, -- their user_id on their server
    username TEXT NOT NULL,
    display_name TEXT,
    avatar_url TEXT, -- URL on their server (not cached locally)
    /* ECDH public key used for user-level E2E encryption of cross-server messages.*/
    ecdh_public_key TEXT,
    last_synced_at TEXT NOT NULL,
    UNIQUE(server_connection_id, remote_user_id)
);

CREATE INDEX IF NOT EXISTS idx_federation_users_server ON federation_users(server_connection_id);
CREATE INDEX IF NOT EXISTS idx_federation_users_remote ON federation_users(server_connection_id, remote_user_id);