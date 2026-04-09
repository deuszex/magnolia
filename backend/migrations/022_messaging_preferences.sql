CREATE TABLE IF NOT EXISTS messaging_preferences (
    user_id TEXT PRIMARY KEY REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    accept_messages INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);