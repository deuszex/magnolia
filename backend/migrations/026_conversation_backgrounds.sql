CREATE TABLE IF NOT EXISTS conversation_backgrounds (
    user_id TEXT NOT NULL REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    conversation_id TEXT NOT NULL REFERENCES conversations(conversation_id) ON DELETE CASCADE,
    media_id TEXT NOT NULL REFERENCES media(media_id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (user_id, conversation_id)
);