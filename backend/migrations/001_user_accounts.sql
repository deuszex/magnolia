CREATE TABLE IF NOT EXISTS user_accounts (
    user_id TEXT PRIMARY KEY,
    email TEXT,
    password_hash TEXT NOT NULL,
    verified INTEGER NOT NULL DEFAULT 0,
    admin INTEGER NOT NULL DEFAULT 0,
    active INTEGER NOT NULL DEFAULT 1,
    email_visible INTEGER NOT NULL DEFAULT 0,
    display_name TEXT,
    username TEXT NOT NULL UNIQUE,
    bio TEXT,
    avatar_media_id TEXT,
    location TEXT,
    website TEXT,
    public_key TEXT,
    /* Store each user's passphrase-encrypted E2E key blob server-side.
    The blob is opaque to the server: it is AES-256-GCM ciphertext wrapped
    around the user's ECDH private key JWK, keyed by a PBKDF2-derived wrapping
    key that the server never sees.  Only the owning user can decrypt it. */
    e2e_key_blob TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_user_accounts_username ON user_accounts(username);

-- Messages received from remote servers are stored with sender_id = '__fed__'
-- and the real sender in remote_sender_qualified_id.
INSERT OR IGNORE INTO user_accounts
    (user_id, email, username, password_hash, verified, admin, active, created_at, updated_at)
VALUES
    ('__fed__', '__fed__@system.internal', '__fed__', '', 0, 0, 0,
    datetime('now'), datetime('now'));