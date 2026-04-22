CREATE TABLE IF NOT EXISTS proxy_accounts (
    proxy_id TEXT PRIMARY KEY,
    paired_user_id TEXT REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    display_name TEXT,
    active INTEGER NOT NULL DEFAULT 0,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    bio TEXT,
    avatar_media_id TEXT,
    hmac_key TEXT,
    public_key TEXT,
    /* Store each user's passphrase-encrypted E2E key blob server-side.
    The blob is opaque to the server: it is AES-256-GCM ciphertext wrapped
    around the user's ECDH private key JWK, keyed by a PBKDF2-derived wrapping
    key that the server never sees.  Only the owning user can decrypt it.
    Use the passphrase on the proxys client to decrypt the key,
    so the proxy is capable of communications.*/
    e2e_key_blob TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL  
);

