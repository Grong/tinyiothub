-- T6：cron_jobs.job_type CHECK 扩展 'approval_timeout'（AI 处置流审批超时扫描）。
-- SQLite 不能改 CHECK——重建表（migration runner 全程 FK OFF，cron_runs
-- 的外键在重建期间不受损）。

CREATE TABLE cron_jobs_new (
    id              TEXT PRIMARY KEY,
    workspace_id    TEXT NOT NULL,
    name            TEXT NOT NULL,
    description     TEXT,
    job_type        TEXT NOT NULL DEFAULT 'shell'
                    CHECK (job_type IN ('shell', 'agent', 'device_command', 'event_retention', 'approval_timeout')),
    cron_expression TEXT NOT NULL,
    config          TEXT NOT NULL DEFAULT '{}',
    timeout_seconds INTEGER DEFAULT 300,
    max_retries     INTEGER DEFAULT 3,
    is_enabled      BOOLEAN NOT NULL DEFAULT true,
    is_running      BOOLEAN NOT NULL DEFAULT false,
    last_run_at     TEXT,
    last_run_status TEXT,
    last_run_error  TEXT,
    next_run_at     TEXT,
    run_count       INTEGER DEFAULT 0,
    success_count   INTEGER DEFAULT 0,
    fail_count      INTEGER DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    created_by      TEXT,
    UNIQUE(workspace_id, name)
);

INSERT INTO cron_jobs_new SELECT * FROM cron_jobs;
DROP TABLE cron_jobs;
ALTER TABLE cron_jobs_new RENAME TO cron_jobs;

CREATE INDEX idx_cron_jobs_workspace ON cron_jobs(workspace_id);
CREATE INDEX idx_cron_jobs_due ON cron_jobs(is_enabled, is_running, next_run_at);
