ALTER TABLE session_projection_snapshots
    ADD COLUMN projection_version INTEGER NOT NULL DEFAULT 1;
