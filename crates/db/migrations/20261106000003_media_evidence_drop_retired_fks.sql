-- Task 19C4: Task and Job ids are workspace-Harness facts.
--
-- The retired platform `agent_tasks` / `device_jobs` tables no longer receive
-- production rows, so the foreign keys on `media_evidence.task_id` and
-- `media_evidence.job_id` can never be satisfied and would reject every
-- Task-/Job-owned evidence row. Media evidence remains a platform telemetry
-- fact; only the stale FKs are removed. The table is rebuilt through the
-- sanctioned `<name>_new` self-copy so existing rows survive; DDL-only.
CREATE TABLE media_evidence_new (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    task_id TEXT,
    run_id TEXT,
    tool_call_id TEXT,
    job_id TEXT,
    device_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_seq INTEGER NOT NULL,
    media_time TEXT NOT NULL,
    mime TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    dedup_key TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);
INSERT INTO media_evidence_new
    (id, workspace_id, task_id, run_id, tool_call_id, job_id, device_id, source_id,
     source_seq, media_time, mime, size_bytes, sha256, storage_path, dedup_key,
     created_at, expires_at)
SELECT id, workspace_id, task_id, run_id, tool_call_id, job_id, device_id, source_id,
       source_seq, media_time, mime, size_bytes, sha256, storage_path, dedup_key,
       created_at, expires_at
FROM media_evidence;
DROP TABLE media_evidence;
ALTER TABLE media_evidence_new RENAME TO media_evidence;
CREATE INDEX idx_media_evidence_task ON media_evidence(task_id, created_at);
CREATE INDEX idx_media_evidence_expiry ON media_evidence(expires_at);
