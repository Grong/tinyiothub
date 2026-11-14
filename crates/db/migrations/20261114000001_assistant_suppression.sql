-- Durable Assistant suppression lifecycle (platform DB).
--
-- Baseline, cursor, registry/config/state versions and the full/audit/sync
-- fences live per (workspace, session). Skipped events themselves remain in
-- the per-workspace Harness DB; these tables never claim cross-DB atomicity.

CREATE TABLE IF NOT EXISTS suppression_state (
    workspace_id             TEXT NOT NULL,
    session_id               TEXT NOT NULL,
    state_version            INTEGER NOT NULL DEFAULT 0,
    config_version           INTEGER NOT NULL DEFAULT 1,
    registry_version         TEXT,
    control_cursor           INTEGER NOT NULL DEFAULT 0,
    baseline_state_id        TEXT,
    baseline_turn_state_json TEXT,
    baseline_observed_at     TEXT,
    baseline_installed_at    TEXT,
    attempt_no               INTEGER NOT NULL DEFAULT 0,
    latest_attempt_id        TEXT,
    latest_outcome           TEXT NOT NULL DEFAULT 'baseline_missing',
    full_attempt_in_progress INTEGER NOT NULL DEFAULT 0,
    full_failed              INTEGER NOT NULL DEFAULT 0,
    audit_in_progress        INTEGER NOT NULL DEFAULT 0,
    schedule_sync_pending    INTEGER NOT NULL DEFAULT 0,
    projection_sync_pending  INTEGER NOT NULL DEFAULT 0,
    notification_pending     INTEGER NOT NULL DEFAULT 0,
    last_error               TEXT,
    created_at               TEXT NOT NULL,
    updated_at               TEXT NOT NULL,
    PRIMARY KEY (workspace_id, session_id)
);

CREATE INDEX IF NOT EXISTS idx_suppression_state_session
    ON suppression_state(session_id);

CREATE TABLE IF NOT EXISTS suppression_idempotency (
    workspace_id        TEXT NOT NULL,
    operation           TEXT NOT NULL,
    key                 TEXT NOT NULL,
    request_fingerprint TEXT NOT NULL,
    planned_wake_at     TEXT,
    attempt_id          TEXT,
    result_json         TEXT,
    state_version       INTEGER NOT NULL DEFAULT 1,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    UNIQUE (workspace_id, operation, key)
);

CREATE INDEX IF NOT EXISTS idx_suppression_idempotency_attempt
    ON suppression_idempotency(attempt_id);

CREATE TABLE IF NOT EXISTS suppression_gate_evaluations (
    artifact_id             TEXT PRIMARY KEY,
    workspace_id            TEXT NOT NULL,
    mode                    TEXT NOT NULL,
    sample_json             TEXT NOT NULL,
    denominators_json       TEXT NOT NULL,
    shadow_estimates_json   TEXT NOT NULL,
    canary_measured_json    TEXT NOT NULL,
    false_skip_review_json  TEXT NOT NULL,
    latency_json            TEXT NOT NULL,
    approver                TEXT NOT NULL,
    content_sha256          TEXT NOT NULL,
    approved_at             TEXT NOT NULL,
    state_version           INTEGER NOT NULL DEFAULT 1,
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_suppression_gate_workspace
    ON suppression_gate_evaluations(workspace_id, created_at);

ALTER TABLE assistant_schedules ADD COLUMN suppression_mode TEXT NOT NULL DEFAULT 'off';
ALTER TABLE assistant_schedules ADD COLUMN reconciliation_seconds INTEGER NOT NULL DEFAULT 1800;
ALTER TABLE assistant_schedules ADD COLUMN suppression_config_version INTEGER NOT NULL DEFAULT 1;
