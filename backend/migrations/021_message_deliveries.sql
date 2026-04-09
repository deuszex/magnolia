CREATE TABLE IF NOT EXISTS message_deliveries (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES messages(message_id) ON DELETE CASCADE,
    recipient_id TEXT NOT NULL REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    delivered_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_message_deliveries_msg ON message_deliveries(message_id);
CREATE INDEX IF NOT EXISTS idx_message_deliveries_recipient ON message_deliveries(recipient_id);