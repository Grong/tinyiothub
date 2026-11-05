-- Task 19A: workspace-backed TaskStore facts.
--
-- The Harness workspace already owned the minimal Task/Run projections used by
-- Workflow, Effect, Command, and simulation paths. The Runtime TaskStore needs
-- the full persisted Task/Run record plus finalize intents in the same
-- workspace DB so the legacy platform writers can be retired without changing
-- the port contract.
--
-- `agent_tasks.state` remains the canonical Harness projection vocabulary
-- (`pending`/`running`/`interrupted`/...); the adapter maps it to
-- `TaskStatus` at the port boundary.
--
-- The new columns are prefixed `runtime_` because some tests apply both the
-- platform and `harness_` migration sets to one database (see
-- 20261105000001_task_wake_failure_reason.sql). The platform `agent_tasks` and
-- `agent_runs` already own the unprefixed names.

ALTER TABLE agent_tasks ADD COLUMN runtime_idempotency_key TEXT NOT NULL DEFAULT '';
ALTER TABLE agent_tasks ADD COLUMN runtime_objective TEXT NOT NULL DEFAULT '';
ALTER TABLE agent_tasks ADD COLUMN runtime_session_key TEXT;
ALTER TABLE agent_tasks ADD COLUMN runtime_created_by TEXT NOT NULL DEFAULT '';
ALTER TABLE agent_tasks ADD COLUMN runtime_created_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z';
ALTER TABLE agent_tasks ADD COLUMN runtime_updated_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z';
ALTER TABLE agent_tasks ADD COLUMN runtime_ended_at TEXT;

-- Workflow-created production Tasks carry no idempotency key; excluding the
-- empty string keeps their coexistence legal while enforcing Runtime replay
-- identity.
CREATE UNIQUE INDEX idx_agent_tasks_production_idempotency
ON agent_tasks (runtime_idempotency_key)
WHERE execution_mode = 'production' AND runtime_idempotency_key <> '';

-- The platform `agent_tasks` has no `state` column (it uses `status`), so the
-- activity index must not reference it; this migration is also applied to
-- combined test databases.
CREATE INDEX idx_agent_tasks_runtime_activity
ON agent_tasks (runtime_updated_at);

ALTER TABLE agent_runs ADD COLUMN runtime_wake_event_id TEXT;
ALTER TABLE agent_runs ADD COLUMN runtime_summary TEXT NOT NULL DEFAULT '';
ALTER TABLE agent_runs ADD COLUMN runtime_report_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE agent_runs ADD COLUMN runtime_outcome TEXT;
ALTER TABLE agent_runs ADD COLUMN runtime_verified INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agent_runs ADD COLUMN runtime_tool_calls INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agent_runs ADD COLUMN runtime_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agent_runs ADD COLUMN runtime_duration_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agent_runs ADD COLUMN runtime_created_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z';
ALTER TABLE agent_runs ADD COLUMN runtime_updated_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z';

CREATE UNIQUE INDEX idx_agent_runs_runtime_task_wake
ON agent_runs (task_id, runtime_wake_event_id)
WHERE runtime_wake_event_id IS NOT NULL;

CREATE INDEX idx_agent_runs_runtime_task_created
ON agent_runs (task_id, runtime_created_at);

CREATE TABLE IF NOT EXISTS task_finalize_intents (
    task_id            TEXT NOT NULL,
    workspace_id       TEXT NOT NULL,
    wake_event_id      TEXT NOT NULL,
    run_id             TEXT NOT NULL,
    report_json        TEXT NOT NULL,
    report_persisted   INTEGER NOT NULL,
    report_emitted     INTEGER NOT NULL,
    undelivered_json   TEXT NOT NULL,
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL,
    PRIMARY KEY (task_id, wake_event_id)
);

CREATE INDEX IF NOT EXISTS idx_task_finalize_intents_workspace
ON task_finalize_intents (workspace_id, task_id);
