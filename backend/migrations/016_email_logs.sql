CREATE TABLE IF NOT EXISTS email_logs (
    email_id TEXT PRIMARY KEY,
    email_type TEXT NOT NULL,
    recipient TEXT NOT NULL,
    subject TEXT NOT NULL,
    sent_at TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'sent',
    related_id TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_email_logs_type ON email_logs(email_type);
CREATE INDEX IF NOT EXISTS idx_email_logs_recipient ON email_logs(recipient);