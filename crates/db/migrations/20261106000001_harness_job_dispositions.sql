-- Task 19C3: unknown-Job dispositions move with the Job into the workspace.
--
-- The platform `runtime_unknown_dispositions` row references the platform
-- `device_jobs` table. Workspace Jobs have no platform parent, so the
-- disposition must live beside the Job. `IF NOT EXISTS` keeps combined test
-- databases (platform + harness migrations applied to one file) legal.
CREATE TABLE IF NOT EXISTS runtime_unknown_dispositions (
    job_id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    task_id TEXT,
    device_id TEXT NOT NULL,
    disposition TEXT NOT NULL,
    recorded_by TEXT NOT NULL,
    recorded_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_runtime_unknown_dispositions_ws
ON runtime_unknown_dispositions(workspace_id);
