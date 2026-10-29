-- Final hardening round for the multi-device Agent Runtime.
--
-- 1. Ledger indexes for the runtime event dedupe (event id) and the ordered
--    task scan. The dedupe expression was previously a post-filter over every
--    hub event of a workspace.
CREATE INDEX IF NOT EXISTS idx_hub_ledger_runtime_event_id
ON hub_ledger(workspace_id, json_extract(payload, '$.eventId'))
WHERE kind = 'runtime.v1';

CREATE INDEX IF NOT EXISTS idx_hub_ledger_runtime_ordered
ON hub_ledger(workspace_id, kind, id);

-- 2. Incremental per-(Task, device) state projection. `deviceFacts` enrichment
--    reads this single row instead of replaying the Task's whole ledger on
--    every device event.
CREATE TABLE IF NOT EXISTS runtime_device_state (
    workspace_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    state_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (workspace_id, task_id, device_id)
);

-- 3. Scope Job/approval idempotency keys by workspace. The original tables
--    declared column-level UNIQUE, which cannot be dropped in place; the tables
--    are new on this branch, so a data-preserving rebuild is safe and keeps the
--    final schema workspace-scoped.
CREATE TABLE device_jobs_scoped (
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
    tool_call_id TEXT,
    driver_call_id TEXT,
    timeout_ms INTEGER NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    progress_revision INTEGER NOT NULL DEFAULT 0
);
INSERT INTO device_jobs_scoped
    (id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
     progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id,
     timeout_ms, started_at, ended_at, progress_revision)
SELECT id, workspace_id, device_id, tool, args_json, state, stop_requested_at,
       progress_json, result_json, task_id, run_id, tool_call_id, driver_call_id,
       timeout_ms, started_at, ended_at, progress_revision
FROM device_jobs;
DROP TABLE device_jobs;
ALTER TABLE device_jobs_scoped RENAME TO device_jobs;
CREATE INDEX idx_device_jobs_task_state ON device_jobs(task_id, state);
CREATE INDEX idx_device_jobs_device_state ON device_jobs(workspace_id, device_id, state);
CREATE UNIQUE INDEX idx_device_jobs_workspace_tool_call
ON device_jobs(workspace_id, tool_call_id)
WHERE tool_call_id IS NOT NULL;

CREATE TABLE task_approvals_scoped (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    task_id TEXT NOT NULL REFERENCES agent_tasks(id),
    run_id TEXT NOT NULL,
    tool_call_id TEXT NOT NULL,
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
    decision_event_id TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
INSERT INTO task_approvals_scoped
    (id, workspace_id, task_id, run_id, tool_call_id, device_id, tool, args_json,
     reason, state, decision, decider_id, requested_at, decided_at, expires_at,
     decision_event_id, updated_at)
SELECT id, workspace_id, task_id, run_id, tool_call_id, device_id, tool, args_json,
       reason, state, decision, decider_id, requested_at, decided_at, expires_at,
       decision_event_id, updated_at
FROM task_approvals;
DROP TABLE task_approvals;
ALTER TABLE task_approvals_scoped RENAME TO task_approvals;
CREATE INDEX idx_task_approvals_task_state
ON task_approvals(workspace_id, task_id, state);
CREATE INDEX idx_task_approvals_expiry
ON task_approvals(expires_at, state);
CREATE UNIQUE INDEX idx_task_approvals_workspace_tool_call
ON task_approvals(workspace_id, tool_call_id);
CREATE UNIQUE INDEX idx_task_approvals_workspace_decision_event
ON task_approvals(workspace_id, decision_event_id);
