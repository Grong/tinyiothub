CREATE TABLE task_approvals (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    task_id TEXT NOT NULL REFERENCES agent_tasks(id),
    run_id TEXT NOT NULL,
    tool_call_id TEXT NOT NULL UNIQUE,
    device_id TEXT NOT NULL,
    tool TEXT NOT NULL,
    args_json TEXT NOT NULL,
    reason TEXT NOT NULL,
    state TEXT NOT NULL,
    decision TEXT,
    decider_id TEXT,
    requested_at TEXT NOT NULL,
    decided_at TEXT,
    expires_at TEXT NOT NULL,
    decision_event_id TEXT NOT NULL UNIQUE,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_task_approvals_task_state
ON task_approvals(workspace_id, task_id, state);
CREATE INDEX idx_task_approvals_expiry
ON task_approvals(expires_at, state);
