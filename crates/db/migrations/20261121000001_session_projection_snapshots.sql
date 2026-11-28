CREATE TABLE IF NOT EXISTS session_projection_snapshots (
    projection_name TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    last_seq INTEGER NOT NULL,
    state_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (projection_name, subject_id)
);
