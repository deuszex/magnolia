-- Known peer servers 
CREATE TABLE IF NOT EXISTS server_connections (
    id TEXT PRIMARY KEY, -- UUID
    address TEXT NOT NULL UNIQUE, -- BASE_URL of peer, e.g. https://other.mysite.com
    display_name TEXT, -- admin-assigned friendly name
    /*Lifecycle: pending_out | pending_in | active | rejected | revoked*/
    status TEXT NOT NULL DEFAULT 'pending_out',

    /*Peer's permanent ML-DSA-87 verifying key (received during handshake).*/
    their_ml_dsa_public_key BLOB,
    /*Peer's ML-KEM-1024 encapsulation key sent with their connection request.*/
    /*Retained so we can re-key without a full new handshake.*/
    their_ml_kem_encap_key BLOB,
    /*Shared 32-byte secret derived from ML-KEM encapsulation, encrypted at rest*/
    /*the same way as ml_dsa_private_key above.*/
    shared_secret BLOB,

    /* Peer's preferences conveyed during/after handshake.*/
    /* 1 = they allow us to mention them when introducing ourselves to new peers.*/
    peer_is_shareable INTEGER NOT NULL DEFAULT 0,
    /* 1 = they want to receive our known-connection list when we introduce ourselves.*/
    peer_wants_discovery INTEGER NOT NULL DEFAULT 1,

    /* Our local admin preferences for this connection.*/
    /* 1 = share our own connections with this peer on introduction.*/
    we_share_connections INTEGER NOT NULL DEFAULT 0,
    /* Admin notes (rename label, private memos).*/
    notes TEXT,

    /* Cached count of local users who have banned this peer server.*/
    local_ban_count INTEGER NOT NULL DEFAULT 0,
    violation_count INTEGER NOT NULL DEFAULT 0,
    peer_request_id TEXT
    last_violation_at TEXT,
    created_at TEXT NOT NULL,
    connected_at TEXT,
    last_seen_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_server_connections_status ON server_connections(status);
CREATE INDEX IF NOT EXISTS idx_server_connections_address ON server_connections(address);