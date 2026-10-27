ALTER TABLE agent_runs ADD COLUMN task_id TEXT;
ALTER TABLE agent_runs ADD COLUMN wake_event_id TEXT;
CREATE INDEX idx_agent_runs_task ON agent_runs(task_id);
CREATE UNIQUE INDEX idx_agent_runs_task_wake
ON agent_runs(task_id, wake_event_id)
WHERE task_id IS NOT NULL AND wake_event_id IS NOT NULL;

CREATE TABLE agent_tasks (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    idempotency_key TEXT NOT NULL,
    objective TEXT NOT NULL,
    session_key TEXT,
    created_by TEXT NOT NULL,
    status TEXT NOT NULL,
    current_run_id TEXT,
    state_version INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    ended_at TEXT
);
CREATE UNIQUE INDEX idx_agent_tasks_idempotency
ON agent_tasks(workspace_id, idempotency_key);
CREATE INDEX idx_agent_tasks_status_activity
ON agent_tasks(workspace_id, status, updated_at);

CREATE TABLE task_wake_outbox (
    task_id TEXT NOT NULL REFERENCES agent_tasks(id),
    event_id TEXT NOT NULL,
    signal_json TEXT NOT NULL,
    dispatch_state TEXT NOT NULL,
    created_at TEXT NOT NULL,
    dispatched_at TEXT,
    resolved_at TEXT,
    PRIMARY KEY(task_id, event_id)
);
CREATE INDEX idx_task_wake_pending
ON task_wake_outbox(dispatch_state, created_at);

ALTER TABLE watch_rules ADD COLUMN owner_task_id TEXT REFERENCES agent_tasks(id);
CREATE INDEX idx_watch_rules_owner_task ON watch_rules(owner_task_id);
