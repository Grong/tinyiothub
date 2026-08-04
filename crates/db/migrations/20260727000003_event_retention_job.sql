-- Event retention: allow 'event_retention' job type + seed the global job
-- (eng-review/CEO expansion X1, 2026-07-27)
--
-- SQLite cannot ALTER a CHECK constraint, so cron_jobs is rebuilt with
-- 'event_retention' added. The seeded job runs the retention purge daily
-- (off-peak, off-:00): occurrence-type events (is_status = 0) older than
-- retention_days are deleted; status rows (is_status = 1) are the live
-- current-state of devices and are NEVER purged by time.

PRAGMA defer_foreign_keys = ON;

CREATE TABLE cron_jobs_new (
    id              TEXT PRIMARY KEY,
    workspace_id    TEXT NOT NULL,
    name            TEXT NOT NULL,
    description     TEXT,
    job_type        TEXT NOT NULL DEFAULT 'shell'
                    CHECK (job_type IN ('shell', 'agent', 'device_command', 'event_retention')),
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

INSERT INTO cron_jobs_new
    (id, workspace_id, name, description, job_type, cron_expression, config,
     timeout_seconds, max_retries, is_enabled, is_running, last_run_at,
     last_run_status, last_run_error, next_run_at, run_count, success_count,
     fail_count, created_at, updated_at, created_by)
SELECT id, workspace_id, name, description, job_type, cron_expression, config,
       timeout_seconds, max_retries, is_enabled, is_running, last_run_at,
       last_run_status, last_run_error, next_run_at, run_count, success_count,
       fail_count, created_at, updated_at, created_by
FROM cron_jobs;

DROP TABLE cron_jobs;
ALTER TABLE cron_jobs_new RENAME TO cron_jobs;

CREATE INDEX IF NOT EXISTS idx_cron_jobs_workspace ON cron_jobs(workspace_id);
CREATE INDEX IF NOT EXISTS idx_cron_jobs_due ON cron_jobs(is_enabled, is_running, next_run_at);

-- Global retention job: daily at 03:17, 90-day default retention for
-- occurrence-type events. 'system' workspace marks platform-owned jobs.
INSERT INTO cron_jobs
    (id, workspace_id, name, description, job_type, cron_expression, config,
     timeout_seconds, max_retries, is_enabled, created_by, created_at, updated_at)
SELECT
    'sys-event-retention',
    'system',
    'Events 保留清理',
    'Delete occurrence-type events (is_status=0) older than retention_days; status rows are never time-purged.',
    'event_retention',
    '0 17 3 * * *',
    '{"retention_days": 90}',
    300,
    3,
    1,
    NULL,
    datetime('now'),
    datetime('now')
WHERE NOT EXISTS (SELECT 1 FROM cron_jobs WHERE id = 'sys-event-retention');
