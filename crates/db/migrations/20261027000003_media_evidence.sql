CREATE TABLE media_evidence (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    task_id TEXT REFERENCES agent_tasks(id),
    run_id TEXT,
    tool_call_id TEXT,
    job_id TEXT REFERENCES device_jobs(id),
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
CREATE INDEX idx_media_evidence_task ON media_evidence(task_id, created_at);
CREATE INDEX idx_media_evidence_expiry ON media_evidence(expires_at);
