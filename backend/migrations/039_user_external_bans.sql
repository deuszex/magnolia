-- Local users banning specific remote users 
-- When banned, the server stops forwarding messages and posts from that
-- remote user to the local user.
CREATE TABLE IF NOT EXISTS user_external_bans (
    local_user_id TEXT NOT NULL REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    server_connection_id TEXT NOT NULL REFERENCES server_connections(id) ON DELETE CASCADE,
    remote_user_id TEXT NOT NULL,
    banned_at TEXT NOT NULL,
    PRIMARY KEY (local_user_id, server_connection_id, remote_user_id)
);

CREATE INDEX IF NOT EXISTS idx_external_bans_local ON user_external_bans(local_user_id);
-- Used when updating local_ban_count on server_connections.
CREATE INDEX IF NOT EXISTS idx_external_bans_server ON user_external_bans(server_connection_id);