-- Server identity (single row, our own ML-DSA keypair) 
CREATE TABLE IF NOT EXISTS server_identity (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    /*ML-DSA-87 signing keypair tied to the server.
    If changed the server will have a different identity and connections will not recognize.
    Private key is AES-256-GCM encrypted when ENCRYPTION_AT_REST_KEY is set.
    otherwise stored as raw bytes (still root-only via DB permissions).*/
    ml_dsa_public_key BLOB NOT NULL,
    ml_dsa_private_key BLOB NOT NULL,
    created_at TEXT NOT NULL
);