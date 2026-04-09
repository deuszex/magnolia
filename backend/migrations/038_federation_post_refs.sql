-- Lazy-load references to remote posts (no content stored server-side) 
-- When a local user's feed would show a remote post, we store a stub here.
-- The actual content is always fetched live from the originating server.
-- On re-fetch, content_hash lets us detect whether the post has changed.
CREATE TABLE IF NOT EXISTS federation_post_refs (
    id TEXT PRIMARY KEY,
    server_connection_id TEXT NOT NULL REFERENCES server_connections(id) ON DELETE CASCADE,
    remote_user_id TEXT NOT NULL,
    remote_post_id TEXT NOT NULL,
    posted_at TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    first_seen_at TEXT NOT NULL,
    content_json TEXT,
    server_address TEXT,
    UNIQUE(server_connection_id, remote_user_id, remote_post_id)
);

CREATE INDEX IF NOT EXISTS idx_fed_post_refs_server ON federation_post_refs(server_connection_id);
CREATE INDEX IF NOT EXISTS idx_fed_post_refs_posted ON federation_post_refs(posted_at);