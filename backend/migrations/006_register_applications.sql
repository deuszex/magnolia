CREATE TABLE IF NOT EXISTS register_applications (
    application_id TEXT PRIMARY KEY,
    email TEXT NOT NULL,
    display_name TEXT,
    message TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    reviewed_at TEXT,
    reviewed_by TEXT REFERENCES user_accounts(user_id)
);

CREATE INDEX IF NOT EXISTS idx_reg_applications_email ON register_applications (email);
CREATE INDEX IF NOT EXISTS idx_reg_applications_status ON register_applications (status);