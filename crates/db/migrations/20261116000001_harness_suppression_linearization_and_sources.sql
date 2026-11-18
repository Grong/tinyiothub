-- Linearize workspace suppression mode changes with skip commits and retain
-- adapter-owned immutable Source bytes across registry rebuilds/restarts.

CREATE TABLE IF NOT EXISTS suppression_workspace_config (
    workspace_id        TEXT PRIMARY KEY,
    mode                TEXT NOT NULL,
    config_version      INTEGER NOT NULL,
    updated_at          TEXT NOT NULL
);

-- Source adapters own these records. The Harness stores only opaque bytes and
-- generic identity/revision/disposition metadata; closed tombstones are not
-- physically released in Phase 1.
CREATE TABLE IF NOT EXISTS suppression_source_entries (
    workspace_id        TEXT NOT NULL,
    session_id          TEXT NOT NULL,
    source_id           TEXT NOT NULL,
    work_key            TEXT NOT NULL,
    revision            TEXT NOT NULL,
    disposition         TEXT NOT NULL,
    payload_hash        TEXT NOT NULL,
    payload_bytes       BLOB NOT NULL,
    updated_at          TEXT NOT NULL,
    PRIMARY KEY (workspace_id, session_id, source_id, work_key)
);

CREATE INDEX IF NOT EXISTS idx_suppression_source_payload
    ON suppression_source_entries(workspace_id, session_id, source_id, payload_hash, revision);

-- Multiple immutable states are retained by hash so an older captured ref is
-- not overwritten by a later capture of the same Source.
CREATE TABLE IF NOT EXISTS suppression_source_states (
    workspace_id        TEXT NOT NULL,
    session_id          TEXT NOT NULL,
    source_id           TEXT NOT NULL,
    state_hash          TEXT NOT NULL,
    revision            TEXT NOT NULL,
    state_bytes         BLOB NOT NULL,
    created_at          TEXT NOT NULL,
    PRIMARY KEY (workspace_id, session_id, source_id, state_hash)
);
