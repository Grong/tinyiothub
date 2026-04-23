-- Create chat_sessions table (for fresh databases) or enhance existing one
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
