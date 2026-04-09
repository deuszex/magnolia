CREATE TABLE IF NOT EXISTS invites (
    invite_id TEXT PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    email TEXT,
    created_by TEXT NOT NULL REFERENCES user_accounts(user_id),
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT,
    used_by_user_id TEXT REFERENCES user_accounts(user_id)
);

CREATE INDEX IF NOT EXISTS idx_invites_token ON invites (token);
CREATE INDEX IF NOT EXISTS idx_invites_created_by ON invites (created_by);