CREATE TABLE IF NOT EXISTS conversations (
    conversation_id TEXT PRIMARY KEY,
    conversation_type TEXT NOT NULL,
    name TEXT,
    -- Allow a local conversation on the receiving server to be linked back to
    -- the originating group's conversation_id so all senders route to the same place.
    remote_group_id TEXT,
    created_by TEXT NOT NULL REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_conversations_remote_group_id ON conversations(remote_group_id);