CREATE TABLE IF NOT EXISTS chat_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_key TEXT NOT NULL UNIQUE,
    agent_id TEXT NOT NULL,
    user_id TEXT,
    workspace_id TEXT,
    label TEXT,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_chat_sessions_agent ON chat_sessions(agent_id);
CREATE INDEX idx_chat_sessions_user ON chat_sessions(user_id);
CREATE INDEX idx_chat_sessions_workspace ON chat_sessions(workspace_id);

CREATE TABLE IF NOT EXISTS chat_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_key TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    run_id TEXT,
    tool_call_id TEXT,
    tool_name TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (session_key) REFERENCES chat_sessions(session_key)
);

CREATE INDEX idx_chat_messages_session ON chat_messages(session_key);
CREATE INDEX idx_chat_messages_run ON chat_messages(run_id);

CREATE TABLE IF NOT EXISTS chat_compacted_sessions (
    session_key TEXT PRIMARY KEY,
    system_messages TEXT NOT NULL,
    summary_message TEXT,
    recent_messages TEXT NOT NULL,
    compacted_at INTEGER NOT NULL,
    original_message_count INTEGER NOT NULL
);
