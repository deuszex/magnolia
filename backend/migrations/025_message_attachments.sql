CREATE TABLE IF NOT EXISTS message_attachments (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES messages(message_id) ON DELETE CASCADE,
    media_id TEXT NOT NULL REFERENCES media(media_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_message_attachments_msg ON message_attachments(message_id);
CREATE INDEX IF NOT EXISTS idx_message_attachments_media ON message_attachments(media_id);