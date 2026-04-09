CREATE TABLE IF NOT EXISTS call_participants (
    id TEXT PRIMARY KEY,
    call_id TEXT NOT NULL REFERENCES calls(call_id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'participant',
    status TEXT NOT NULL DEFAULT 'ringing',
    joined_at TEXT,
    left_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_call_participants_call ON call_participants(call_id);
CREATE INDEX IF NOT EXISTS idx_call_participants_user ON call_participants(user_id);
