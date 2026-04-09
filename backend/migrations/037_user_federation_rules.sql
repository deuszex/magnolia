-- Per-server allow/deny rules within a user's whitelist or blacklist 
CREATE TABLE IF NOT EXISTS user_federation_rules (
    user_id TEXT NOT NULL REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    server_connection_id TEXT NOT NULL REFERENCES server_connections(id) ON DELETE CASCADE,
    /* sharing | post_sharing */
    rule_type TEXT NOT NULL,
    /* allow | deny */
    effect TEXT NOT NULL,
    PRIMARY KEY (user_id, server_connection_id, rule_type)
);