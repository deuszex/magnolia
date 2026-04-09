-- Local users' federation sharing preferences 
CREATE TABLE IF NOT EXISTS user_federation_settings (
    user_id TEXT PRIMARY KEY REFERENCES user_accounts(user_id) ON DELETE CASCADE,
    /* off | whitelist | blacklist
    off : not shared with any peer server (default)
    whitelist: only shared with servers explicitly listed as allowed below
    blacklist: shared with all peer servers except those listed as denied below*/
    sharing_mode TEXT NOT NULL DEFAULT 'off',
    /* Same as above but for post sharing.*/
    post_sharing_mode TEXT NOT NULL DEFAULT 'off'
);
