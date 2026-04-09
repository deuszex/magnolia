CREATE TABLE IF NOT EXISTS posts (
    post_id TEXT PRIMARY KEY,
    author_id TEXT NOT NULL REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    is_published INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_posts_author_id ON posts(author_id);
CREATE INDEX IF NOT EXISTS idx_posts_is_published ON posts(is_published);