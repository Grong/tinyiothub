CREATE TABLE projection_snapshots (
    projection_name    TEXT NOT NULL,
    subject_id         TEXT NOT NULL,
    last_seq           INTEGER NOT NULL,
    projection_version INTEGER NOT NULL,
    state_version      INTEGER NOT NULL,
    state_json         TEXT NOT NULL,
    updated_at         TEXT NOT NULL,
    PRIMARY KEY (projection_name, subject_id)
);
