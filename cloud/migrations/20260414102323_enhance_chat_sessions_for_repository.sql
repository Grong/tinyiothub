-- Create chat_sessions table (for fresh databases, must come before ALTER)
CREATE TABLE IF NOT EXISTS chat_sessions (
    session_key TEXT PRIMARY KEY,
    workspace_id TEXT,
    agent_id TEXT,
    label TEXT,
    created_at INTEGER,
    updated_at INTEGER,
    metadata TEXT NOT NULL DEFAULT '{}'
);

-- Add missing columns to existing chat_sessions table (no-op if columns already exist via CREATE)
-- Note: SQLite does not support IF NOT EXISTS for ALTER, so these are only safe
-- when the table was created by an earlier migration without these columns.
-- For fresh databases, the CREATE above already includes them.
-- For existing databases that already have these columns, remove the .bak and skip this migration.

CREATE INDEX IF NOT EXISTS idx_chat_sessions_workspace ON chat_sessions(workspace_id);

-- Create chat_messages table (for fresh databases, must come before ALTER)
CREATE TABLE IF NOT EXISTS chat_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_key TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    timestamp INTEGER,
    run_id TEXT,
    tool_call_id TEXT,
    tool_name TEXT,
    FOREIGN KEY (session_key) REFERENCES chat_sessions(session_key) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_chat_messages_session ON chat_messages(session_key);

-- Table for compacted session data
CREATE TABLE IF NOT EXISTS chat_compacted_sessions (
    session_key TEXT PRIMARY KEY,
    system_messages TEXT NOT NULL,
    summary_message TEXT,
    recent_messages TEXT NOT NULL,
    compacted_at INTEGER NOT NULL,
    original_message_count INTEGER NOT NULL
);
