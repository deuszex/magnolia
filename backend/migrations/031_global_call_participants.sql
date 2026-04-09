-- Global voice call: always-on, no conversation FK required
CREATE TABLE IF NOT EXISTS global_call_participants (
    user_id TEXT PRIMARY KEY,
    joined_at TEXT NOT NULL
);