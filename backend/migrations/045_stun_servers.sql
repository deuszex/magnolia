-- Admin-configurable STUN/TURN server list.
-- Each row is one ICE server URL.  TURN entries may include username + credential.
-- last_status is updated periodically by the background health-check service.
CREATE TABLE IF NOT EXISTS stun_servers (
    id              TEXT    PRIMARY KEY,
    url             TEXT    NOT NULL,           -- e.g. stun:stun.l.google.com:19302
    username        TEXT,                       -- TURN only
    credential      TEXT,                       -- TURN only
    enabled         INTEGER NOT NULL DEFAULT 1,
    last_checked_at TEXT,
    last_status     TEXT    NOT NULL DEFAULT 'unknown', -- ok | unreachable | unknown
    created_at      TEXT    NOT NULL,
    updated_at      TEXT    NOT NULL
);

INSERT OR IGNORE INTO stun_servers 
    (id, url, created_at, updated_at)
VALUES 
    ("google", "stun:stun.l.google.com:19302", datetime('now'), datetime('now'));
