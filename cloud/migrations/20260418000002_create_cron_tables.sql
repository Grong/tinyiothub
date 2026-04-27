PRAGMA foreign_keys = ON;

-- ============================================================
-- Cron Jobs Tables
-- ============================================================

-- ------------------------------------------------------------
-- cron_jobs: stores scheduled job definitions
-- ------------------------------------------------------------
CREATE TABLE IF NOT EXISTS cron_jobs (
    id              TEXT PRIMARY KEY,
    workspace_id    TEXT NOT NULL,
    name            TEXT NOT NULL,
    description     TEXT,
    job_type        TEXT NOT NULL DEFAULT 'shell'
                        CHECK (job_type IN ('shell', 'agent', 'device_command')),
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

-- ------------------------------------------------------------
-- cron_runs: stores execution history for each job
-- ------------------------------------------------------------
CREATE TABLE IF NOT EXISTS cron_runs (
    id            TEXT PRIMARY KEY,
    job_id        TEXT NOT NULL,
    workspace_id  TEXT NOT NULL,
    started_at    TEXT NOT NULL,
    ended_at      TEXT,
    duration_ms   INTEGER,
    status        TEXT NOT NULL
                      CHECK (status IN ('pending', 'running', 'success', 'failed')),
    output        TEXT,
    error_message TEXT,
    trigger_type  TEXT NOT NULL DEFAULT 'schedule',
    triggered_by  TEXT,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (job_id) REFERENCES cron_jobs(id) ON DELETE CASCADE
);

-- ============================================================
-- Indexes
-- ============================================================

-- Query jobs that are due to run
CREATE INDEX IF NOT EXISTS idx_cron_jobs_due
    ON cron_jobs(is_enabled, is_running, next_run_at);

-- List jobs within a workspace
CREATE INDEX IF NOT EXISTS idx_cron_jobs_workspace
    ON cron_jobs(workspace_id);

-- Look up runs for a specific job
CREATE INDEX IF NOT EXISTS idx_cron_runs_job_id
    ON cron_runs(job_id);

-- Filter runs by status
CREATE INDEX IF NOT EXISTS idx_cron_runs_status
    ON cron_runs(status);

-- Order runs by start time
CREATE INDEX IF NOT EXISTS idx_cron_runs_started
    ON cron_runs(started_at);

-- List runs within a workspace
CREATE INDEX IF NOT EXISTS idx_cron_runs_workspace
    ON cron_runs(workspace_id);
