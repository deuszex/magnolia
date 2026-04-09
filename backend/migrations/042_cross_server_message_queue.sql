-- Cross-server message log (minimal — routing state only) 
-- The encrypted payload is never stored here. this is purely for delivery
-- acknowledgement tracking so the hub can retry failed fan-outs.
-- Federated message delivery tracking.
--
-- messages.federated_status: NULL = local-only message (no federation needed),
--   'pending'   = queued, not yet acknowledged by the remote server,
--   'delivered' = remote server returned 200 OK.
--
-- Only set on outbound federated messages (sender_id != '__fed__').
CREATE TABLE IF NOT EXISTS cross_server_message_queue (
    -- Stable ID echoed in the envelope so the receiver can deduplicate retries.
    id                      TEXT PRIMARY KEY,
    -- FK back to the originating local message for status updates.
    message_id              TEXT NOT NULL REFERENCES messages(message_id) ON DELETE CASCADE,
    -- Which peer this needs to be delivered to.
    target_server_id        TEXT NOT NULL REFERENCES server_connections(id) ON DELETE CASCADE,
    -- Inner payload fields — stored clear so we can re-encrypt on retry.
    recipient_user_id       TEXT NOT NULL,
    sender_qualified_id     TEXT NOT NULL,
    conversation_id         TEXT NOT NULL,
    encrypted_content       TEXT NOT NULL,
    -- Store conversation context in the delivery queue so retries can reconstruct
    -- the correct envelope type (group messages must not be re-routed as DMs).
    conversation_type       TEXT NOT NULL DEFAULT 'direct',
    group_name              TEXT,
    sent_at                 TEXT NOT NULL,
    -- JSON array of FederatedMediaRef objects (may be '[]').
    attachments_json        TEXT NOT NULL DEFAULT '[]',
    -- 'pending' | 'delivered'
    delivery_status         TEXT NOT NULL DEFAULT 'pending',
    attempts                INTEGER NOT NULL DEFAULT 0,
    created_at              TEXT NOT NULL,
    delivered_at            TEXT,
    last_attempt_at         TEXT
);
CREATE INDEX IF NOT EXISTS idx_cs_msg_queue_status   ON cross_server_message_queue(delivery_status);
CREATE INDEX IF NOT EXISTS idx_cs_msg_queue_server   ON cross_server_message_queue(target_server_id, delivery_status);
CREATE INDEX IF NOT EXISTS idx_cs_msg_queue_message  ON cross_server_message_queue(message_id);
