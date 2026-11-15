-- Server-recorded evidence for the Assistant suppression rollout gate.
--
-- One row per completed suppression-aware Turn (scheduled or manual). Gate
-- artifacts are derived from these rows; model/tool counts are never typed by
-- an operator. The primary key makes a Turn replay idempotent.

CREATE TABLE IF NOT EXISTS suppression_observations (
    workspace_id          TEXT NOT NULL,
    session_id            TEXT NOT NULL,
    turn_id               TEXT NOT NULL,
    attempt_id            TEXT NOT NULL,
    mode                  TEXT NOT NULL,
    observed_at           TEXT NOT NULL,
    would_skip            INTEGER NOT NULL,
    vetoes_json           TEXT NOT NULL,
    model_calls           INTEGER NOT NULL,
    tool_calls            INTEGER NOT NULL,
    preflight_latency_ms  INTEGER NOT NULL,
    registry_version      TEXT NOT NULL,
    created_at            TEXT NOT NULL,
    PRIMARY KEY (workspace_id, turn_id)
);

CREATE INDEX IF NOT EXISTS idx_suppression_observations_mode_time
    ON suppression_observations(workspace_id, mode, observed_at);
