CREATE TABLE commands (
    seq            INTEGER PRIMARY KEY AUTOINCREMENT,
    command_id     TEXT NOT NULL UNIQUE,
    kind           TEXT NOT NULL,
    target_task_id TEXT NOT NULL,
    target_run_id  TEXT NOT NULL,
    actor          TEXT NOT NULL,
    payload_json   TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    expires_at     TEXT,
    state          TEXT NOT NULL
);

CREATE INDEX idx_commands_target_state_seq
    ON commands (target_task_id, state, seq);

CREATE INDEX idx_commands_requested_expiry
    ON commands (expires_at, seq)
    WHERE state = 'requested';
