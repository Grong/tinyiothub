CREATE TABLE session_events (
    seq            INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id       TEXT NOT NULL UNIQUE,
    session_id     TEXT NOT NULL,
    execution_mode TEXT NOT NULL,
    run_id         TEXT,
    task_id        TEXT,
    type           TEXT NOT NULL,
    occurred_at    TEXT NOT NULL,
    cause_id       INTEGER,
    schema_v       INTEGER NOT NULL,
    payload_digest TEXT NOT NULL,
    payload_json   TEXT NOT NULL
);

CREATE INDEX idx_session_events_mode_session_seq
    ON session_events (execution_mode, session_id, seq);
CREATE INDEX idx_session_events_mode_run_seq
    ON session_events (execution_mode, run_id, seq);
CREATE INDEX idx_session_events_mode_task_seq
    ON session_events (execution_mode, task_id, seq);
