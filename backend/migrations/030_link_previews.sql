-- Cached link preview metadata (OG tags fetched from external URLs)
CREATE TABLE IF NOT EXISTS link_previews (
    url TEXT PRIMARY KEY,
    title TEXT,
    description TEXT,
    image_url TEXT,
    domain TEXT,
    fetched_at TEXT NOT NULL
);