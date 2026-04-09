/* CHECK (id=1) so that only one entry can be in the table */
CREATE TABLE IF NOT EXISTS email_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    smtp_host TEXT NOT NULL DEFAULT '',
    smtp_port INTEGER NOT NULL DEFAULT 587,
    smtp_username TEXT NOT NULL DEFAULT '',
    smtp_password TEXT NOT NULL DEFAULT '',
    smtp_from TEXT NOT NULL DEFAULT '',
    smtp_secure TEXT NOT NULL DEFAULT 'tls',
    high_value_enabled INTEGER NOT NULL DEFAULT 0,
    high_value_threshold INTEGER NOT NULL DEFAULT 10000,
    high_value_recipient TEXT NOT NULL DEFAULT '',
    pending_delivery_enabled INTEGER NOT NULL DEFAULT 0,
    pending_delivery_schedules TEXT NOT NULL DEFAULT '[]',
    pending_delivery_recipient TEXT NOT NULL DEFAULT '',
    pending_delivery_include_products INTEGER NOT NULL DEFAULT 1,
    invoice_email_enabled INTEGER NOT NULL DEFAULT 0,
    invoice_email_trigger TEXT NOT NULL DEFAULT 'processing',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT OR IGNORE INTO email_settings (id, created_at, updated_at) VALUES (1, datetime('now'), datetime('now'));