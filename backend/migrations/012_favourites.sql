CREATE TABLE IF NOT EXISTS favourites (
    favourite_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    product_id TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_favourites_user_id ON favourites(user_id);