-- Cross-server group chats 
-- When is_host = 1 this server manages the group and fans messages out.
-- When is_host = 0 this server is a participant. host_server_id identifies who runs it.
CREATE TABLE IF NOT EXISTS cross_server_groups (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    is_host INTEGER NOT NULL DEFAULT 1,
    host_server_id TEXT REFERENCES server_connections(id),
    /* Group's canonical ID on the host server (needed for non-host participants).*/
    host_group_id TEXT,
    created_by_user_id TEXT REFERENCES user_accounts(user_id),
    created_at TEXT NOT NULL
);
