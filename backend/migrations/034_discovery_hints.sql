-- Discovery hints (peers our connections have told us about) 
CREATE TABLE IF NOT EXISTS discovery_hints (
    id TEXT PRIMARY KEY,
    learned_from_server_id TEXT NOT NULL REFERENCES server_connections(id) ON DELETE CASCADE,
    address TEXT NOT NULL,
    /* suggested | dismissed | connecting */
    status TEXT NOT NULL DEFAULT 'suggested',
    created_at TEXT NOT NULL,
    UNIQUE(learned_from_server_id, address)
);

CREATE INDEX IF NOT EXISTS idx_discovery_hints_status ON discovery_hints(status);