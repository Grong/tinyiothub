CREATE TABLE device_jobs (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    device_id TEXT NOT NULL,
    tool TEXT NOT NULL,
    args_json TEXT NOT NULL,
    state TEXT NOT NULL,
    stop_requested_at TEXT,
    progress_json TEXT,
    result_json TEXT,
    task_id TEXT REFERENCES agent_tasks(id),
    run_id TEXT,
    tool_call_id TEXT UNIQUE,
    driver_call_id TEXT,
    timeout_ms INTEGER NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT
);
CREATE INDEX idx_device_jobs_task_state ON device_jobs(task_id, state);
CREATE INDEX idx_device_jobs_device_state ON device_jobs(workspace_id, device_id, state);
