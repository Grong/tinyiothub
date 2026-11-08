CREATE TABLE IF NOT EXISTS assistant_schedules (
    workspace_id     TEXT PRIMARY KEY,
    assistant_id     TEXT NOT NULL,
    interval_seconds INTEGER NOT NULL DEFAULT 300,
    enabled          INTEGER NOT NULL DEFAULT 1,
    prompt           TEXT NOT NULL DEFAULT '',
    next_wake_at     TEXT,
    last_wake_at     TEXT,
    last_outcome     TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_assistant_schedules_due
    ON assistant_schedules(enabled, next_wake_at);
