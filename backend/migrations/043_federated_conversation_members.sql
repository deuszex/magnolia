-- Tracks remote users (on other servers) who are participants in local
-- conversations. Complementary to conversation_members (local users only).
CREATE TABLE IF NOT EXISTS federated_conversation_members (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(conversation_id) ON DELETE CASCADE,
    server_connection_id TEXT NOT NULL REFERENCES server_connections(id) ON DELETE CASCADE,
    remote_user_id TEXT NOT NULL,
    remote_qualified_id TEXT NOT NULL, -- "username@server"
    joined_at TEXT NOT NULL,
    UNIQUE(conversation_id, server_connection_id, remote_user_id)
);

CREATE INDEX IF NOT EXISTS idx_fed_conv_members_conv
    ON federated_conversation_members(conversation_id);
CREATE INDEX IF NOT EXISTS idx_fed_conv_members_server
    ON federated_conversation_members(server_connection_id);

