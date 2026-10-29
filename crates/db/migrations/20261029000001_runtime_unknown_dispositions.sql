-- T8 rollout drain: an administrator records the physical disposition of an
-- unknown Job outcome before it stops blocking shutdown. The Job row is never
-- rewritten and side effects are never replayed; this table only records the
-- human decision that the physical state was inspected and resolved.
CREATE TABLE runtime_unknown_dispositions (
    job_id TEXT PRIMARY KEY REFERENCES device_jobs(id),
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    task_id TEXT,
    device_id TEXT NOT NULL,
    disposition TEXT NOT NULL,
    recorded_by TEXT NOT NULL,
    recorded_at TEXT NOT NULL
);
CREATE INDEX idx_runtime_unknown_dispositions_ws
ON runtime_unknown_dispositions(workspace_id);
