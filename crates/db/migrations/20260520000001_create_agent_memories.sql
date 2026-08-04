-- Agent memories table (replaces device_memory + AgentMemoryItem)
CREATE TABLE IF NOT EXISTS agent_memories (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    zone TEXT NOT NULL DEFAULT 'general',
    content TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'user',
    confidence TEXT NOT NULL DEFAULT 'medium',
    tags TEXT NOT NULL DEFAULT '[]',
    pinned INTEGER NOT NULL DEFAULT 0,
    supersedes TEXT,
    device_id TEXT,
    snapshot_data TEXT,
    snapshot_time INTEGER,
    effectiveness REAL NOT NULL DEFAULT 1.0,
    load_count INTEGER NOT NULL DEFAULT 0,
    reference_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_memories_ws_agent ON agent_memories(workspace_id, agent_id);
CREATE INDEX IF NOT EXISTS idx_memories_zone ON agent_memories(workspace_id, agent_id, zone);
CREATE INDEX IF NOT EXISTS idx_memories_pinned ON agent_memories(workspace_id, agent_id, pinned);
CREATE INDEX IF NOT EXISTS idx_memories_effectiveness ON agent_memories(workspace_id, agent_id, effectiveness DESC);
CREATE INDEX IF NOT EXISTS idx_memories_source ON agent_memories(workspace_id, agent_id, source);

-- Reflection queue for deferred curation
CREATE TABLE IF NOT EXISTS reflection_queue (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    session_key TEXT NOT NULL,
    candidate_type TEXT NOT NULL,
    candidate_data TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    reviewed_at TEXT,
    reviewer_note TEXT
);

CREATE INDEX IF NOT EXISTS idx_reflection_queue_status
    ON reflection_queue(workspace_id, agent_id, status);

-- Audit log for all reflection actions
CREATE TABLE IF NOT EXISTS reflection_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT,
    label TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_reflection_log_session
    ON reflection_log(session_id, created_at DESC);

-- Data migration: move existing device_memory rows into agent_memories
INSERT INTO agent_memories (id, workspace_id, agent_id, zone, content, source, confidence, tags, device_id, snapshot_data, snapshot_time, created_at, updated_at)
SELECT
    hex(randomblob(16)),
    workspace_id,
    agent_id,
    'general',
    snapshot_data,
    'device_snapshot',
    'medium',
    '["device"]',
    device_id,
    snapshot_data,
    snapshot_time,
    COALESCE(created_at, datetime('now')),
    COALESCE(created_at, datetime('now'))
FROM device_memory
WHERE NOT EXISTS (SELECT 1 FROM agent_memories WHERE agent_memories.device_id = device_memory.device_id AND agent_memories.source = 'device_snapshot');
