CREATE TABLE IF NOT EXISTS user_events (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    category TEXT NOT NULL DEFAULT 'system',
    event_type TEXT NOT NULL,
    priority TEXT NOT NULL DEFAULT 'info',
    title TEXT NOT NULL,
    body TEXT NOT NULL DEFAULT '',
    metadata TEXT,
    viewed INTEGER NOT NULL DEFAULT 0,
    viewed_at TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES user_accounts(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_user_events_user
    ON user_events(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_user_events_unread
    ON user_events(user_id, viewed, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_user_events_category
    ON user_events(user_id, category, created_at DESC);

-- Per-user notification preferences
CREATE TABLE IF NOT EXISTS user_event_prefs (
    user_id TEXT PRIMARY KEY,
    disabled_categories TEXT NOT NULL DEFAULT '[]',
    FOREIGN KEY (user_id) REFERENCES user_accounts(user_id) ON DELETE CASCADE
);
