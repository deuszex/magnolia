CREATE TABLE IF NOT EXISTS calls (
    call_id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(conversation_id) ON DELETE CASCADE,
    initiated_by TEXT NOT NULL REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    call_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'ringing',
    started_at TEXT,
    ended_at TEXT,
    duration_seconds INTEGER,
    is_open INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_calls_conversation ON calls(conversation_id);
CREATE INDEX IF NOT EXISTS idx_calls_status ON calls(status);