CREATE TABLE IF NOT EXISTS comments (
    comment_id TEXT PRIMARY KEY,
    post_id TEXT NOT NULL REFERENCES posts(post_id) ON DELETE CASCADE,
    author_id TEXT NOT NULL REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    parent_comment_id TEXT REFERENCES comments(comment_id) ON DELETE CASCADE,
    content_type TEXT NOT NULL DEFAULT 'text',
    content TEXT NOT NULL,
    media_path TEXT,
    is_deleted INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_comments_post_id ON comments(post_id);
CREATE INDEX IF NOT EXISTS idx_comments_author_id ON comments(author_id);
CREATE INDEX IF NOT EXISTS idx_comments_parent ON comments(parent_comment_id);