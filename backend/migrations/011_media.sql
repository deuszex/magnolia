CREATE TABLE IF NOT EXISTS media (
    media_id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    media_type TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    thumbnail_path TEXT,
    filename TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    file_size INTEGER NOT NULL DEFAULT 0,
    duration_seconds INTEGER,
    width INTEGER,
    height INTEGER,
    file_hash TEXT NOT NULL,
    description TEXT,
    tags TEXT,
    encryption_nonce TEXT,
    is_deleted INTEGER NOT NULL DEFAULT 0,
    -- Federated media fields (NULL = local upload)
    -- origin_server: base_url of the peer that owns this file
    -- origin_media_id: the media_id on the origin server
    -- is_cached: 0 = stub row, file not yet fetched, 1 = file present locally (default for local uploads)
    -- is_fetching: 1 = a fetch is in progress, prevents duplicate concurrent fetches
    origin_server TEXT,
    origin_media_id TEXT,
    is_cached INTEGER NOT NULL DEFAULT 1,
    is_fetching INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_media_owner_id ON media(owner_id);
CREATE INDEX IF NOT EXISTS idx_media_type ON media(media_type);
CREATE INDEX IF NOT EXISTS idx_media_hash ON media(file_hash);
CREATE INDEX IF NOT EXISTS idx_media_origin ON media(origin_server, origin_media_id);