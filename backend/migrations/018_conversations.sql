CREATE TABLE IF NOT EXISTS conversations (
    conversation_id TEXT PRIMARY KEY,
    conversation_type TEXT NOT NULL,
    name TEXT,
    -- Allow a local conversation on the receiving server to be linked back to
    -- the originating group's conversation_id so all senders route to the same place.
    remote_group_id TEXT,
        -- If created by a proxy uses the __proxy__ user, and the actual creator is
    -- in proxy_creator_id 
    created_by TEXT NOT NULL REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    proxy_creator_id TEXT REFERENCES proxy_accounts(proxy_id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_conversations_remote_group_id ON conversations(remote_group_id);