CREATE TABLE IF NOT EXISTS user_blocks (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    blocked_user_id TEXT NOT NULL REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    UNIQUE(user_id, blocked_user_id)
);

CREATE INDEX IF NOT EXISTS idx_user_blocks_user ON user_blocks(user_id);
CREATE INDEX IF NOT EXISTS idx_user_blocks_blocked ON user_blocks(blocked_user_id);