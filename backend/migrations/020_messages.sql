CREATE TABLE IF NOT EXISTS messages (
    message_id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(conversation_id) ON DELETE CASCADE,
    sender_id TEXT NOT NULL REFERENCES user_accounts(user_id) ON DELETE SET NULL,
    proxy_sender_id TEXT REFERENCES proxy_accounts(proxy_id) ON DELETE SET NULL,
    -- NULL for local messages. "username@server" for messages received via S2S.
    remote_sender_qualified_id TEXT,
    federated_status TEXT,
    encrypted_content TEXT NOT NULL,
    content_nonce TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_conv ON messages(conversation_id);
CREATE INDEX IF NOT EXISTS idx_messages_sender ON messages(sender_id);
CREATE INDEX IF NOT EXISTS idx_messages_proxy_sender ON messages(proxy_sender_id);
