CREATE TABLE IF NOT EXISTS known_devices (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    fingerprint TEXT NOT NULL,
    ip_address TEXT,
    user_agent TEXT,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_known_devices_user_fingerprint
    ON known_devices(user_id, fingerprint);
CREATE INDEX IF NOT EXISTS idx_known_devices_user
    ON known_devices(user_id);
