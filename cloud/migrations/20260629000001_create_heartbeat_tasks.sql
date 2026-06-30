CREATE TABLE IF NOT EXISTS heartbeat_tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    priority TEXT NOT NULL DEFAULT 'low',
    text TEXT NOT NULL,
    paused INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(workspace_id, id)
);

CREATE INDEX IF NOT EXISTS idx_heartbeat_tasks_workspace
    ON heartbeat_tasks(workspace_id);
