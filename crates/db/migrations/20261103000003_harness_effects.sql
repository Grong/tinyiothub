CREATE TABLE effect_intents (
    effect_id       TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL,
    task_id         TEXT NOT NULL,
    run_id          TEXT NOT NULL,
    tool_call_id    TEXT NOT NULL,
    tool_name       TEXT NOT NULL,
    request_json    TEXT NOT NULL,
    request_digest  TEXT NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    external_ref    TEXT,
    state           TEXT NOT NULL,
    recovery_from   TEXT,
    state_version   INTEGER NOT NULL,
    result_json     TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    UNIQUE(task_id, tool_call_id)
);

CREATE INDEX idx_effect_intents_active
    ON effect_intents (state, updated_at)
    WHERE state IN ('prepared', 'dispatching', 'cancelling', 'unknown');

CREATE TABLE IF NOT EXISTS device_jobs (
    id                TEXT PRIMARY KEY,
    workspace_id      TEXT NOT NULL,
    device_id         TEXT NOT NULL,
    tool              TEXT NOT NULL,
    args_json         TEXT NOT NULL,
    state             TEXT NOT NULL,
    stop_requested_at TEXT,
    progress_json     TEXT,
    result_json       TEXT,
    task_id           TEXT,
    run_id            TEXT,
    tool_call_id      TEXT,
    driver_call_id    TEXT,
    timeout_ms        INTEGER NOT NULL,
    started_at        TEXT NOT NULL,
    ended_at          TEXT,
    progress_revision INTEGER NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_device_jobs_workspace_tool_call
    ON device_jobs (workspace_id, tool_call_id)
    WHERE tool_call_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_device_jobs_task_state
    ON device_jobs (task_id, state);

CREATE TABLE IF NOT EXISTS task_wake_outbox (
    task_id        TEXT NOT NULL,
    event_id       TEXT NOT NULL,
    signal_json    TEXT NOT NULL,
    dispatch_state TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    dispatched_at  TEXT,
    resolved_at    TEXT,
    PRIMARY KEY(task_id, event_id)
);

CREATE INDEX IF NOT EXISTS idx_task_wake_pending
    ON task_wake_outbox (dispatch_state, created_at);

CREATE TABLE harness_transitions (
    transition_id   TEXT PRIMARY KEY,
    effect_id       TEXT NOT NULL,
    state_version   INTEGER NOT NULL,
    transition_json TEXT NOT NULL,
    created_at      TEXT NOT NULL
);

CREATE INDEX idx_harness_transitions_effect
    ON harness_transitions (effect_id, state_version);

CREATE TABLE IF NOT EXISTS decisions (
    decision_id         TEXT PRIMARY KEY,
    kind                TEXT NOT NULL,
    state               TEXT NOT NULL,
    session_id          TEXT NOT NULL,
    run_id              TEXT NOT NULL,
    task_id             TEXT NOT NULL,
    effect_id           TEXT,
    subject_json        TEXT NOT NULL,
    options_json        TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    expires_at          TEXT,
    answer              TEXT,
    answer_actor        TEXT,
    answer_event_id     TEXT,
    answer_evidence_json TEXT,
    consumed_at         TEXT,
    consumed_by         TEXT,
    CHECK (kind != 'effect_reconciliation' OR expires_at IS NULL)
);

CREATE INDEX idx_decisions_effect
    ON decisions (effect_id, kind, state);
CREATE INDEX idx_decisions_expiry
    ON decisions (expires_at, state)
    WHERE kind = 'tool_approval' AND state = 'requested';

CREATE TABLE IF NOT EXISTS agent_tasks (
    id            TEXT PRIMARY KEY,
    run_id        TEXT,
    state         TEXT NOT NULL,
    state_version INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_runs (
    id      TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    state   TEXT NOT NULL
);
