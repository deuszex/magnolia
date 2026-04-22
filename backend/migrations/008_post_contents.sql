CREATE TABLE IF NOT EXISTS post_contents (
    content_id TEXT PRIMARY KEY,
    post_id TEXT NOT NULL REFERENCES posts(post_id) ON DELETE CASCADE,
    content_type TEXT NOT NULL,
    display_order INTEGER NOT NULL DEFAULT 0,
    content TEXT NOT NULL,
    thumbnail_path TEXT,
    original_filename TEXT,
    mime_type TEXT,
    file_size INTEGER,
    content_nonce TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_post_contents_post_id ON post_contents(post_id);