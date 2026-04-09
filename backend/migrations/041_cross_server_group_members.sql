-- Members of cross-server groups 
CREATE TABLE IF NOT EXISTS cross_server_group_members (
    id TEXT PRIMARY KEY,
    group_id TEXT NOT NULL REFERENCES cross_server_groups(id) ON DELETE CASCADE,
    /* local | remote*/
    member_type TEXT NOT NULL,
    /* Populated when member_type = 'local'*/
    local_user_id TEXT REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    /* Populated when member_type = 'remote' (references our local cache of that user)*/
    federation_user_id TEXT REFERENCES federation_users(id) ON DELETE CASCADE,
    joined_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cs_group_members_group ON cross_server_group_members(group_id);
CREATE INDEX IF NOT EXISTS idx_cs_group_members_local ON cross_server_group_members(local_user_id);
CREATE INDEX IF NOT EXISTS idx_cs_group_members_fed ON cross_server_group_members(federation_user_id);