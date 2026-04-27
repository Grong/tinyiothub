-- Add missing columns to existing chat_sessions table (from storage crate migration)
ALTER TABLE chat_sessions ADD COLUMN workspace_id TEXT;
ALTER TABLE chat_sessions ADD COLUMN metadata TEXT NOT NULL DEFAULT '{}';

-- Recreate chat_sessions with full schema for fresh databases
-- Note: this only runs if the table does not yet exist
CREATE TABLE IF NOT EXISTS chat_sessions (
    session_key TEXT PRIMARY KEY,
    workspace_id TEXT,
    agent_id TEXT,
    label TEXT,
    created_at INTEGER,
    updated_at INTEGER,
    metadata TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_chat_sessions_workspace ON chat_sessions(workspace_id);

-- Add missing columns to existing chat_messages table
ALTER TABLE chat_messages ADD COLUMN tool_call_id TEXT;
ALTER TABLE chat_messages ADD COLUMN tool_name TEXT;

-- Create chat_messages table (for fresh databases)
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
    system_messages TEXT NOT NULL,  -- JSON array of ChatMessage
    summary_message TEXT,           -- JSON object of ChatMessage or null
    recent_messages TEXT NOT NULL,  -- JSON array of ChatMessage
    compacted_at INTEGER NOT NULL,
    original_message_count INTEGER NOT NULL
);
