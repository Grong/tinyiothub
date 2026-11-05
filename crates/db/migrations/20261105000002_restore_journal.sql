-- R29: crash-safe workspace restore journal.
--
-- The journal is a platform-registry row (one per workspace) so a crashed
-- restore can be classified before any workspace DB is opened. Paths are
-- recorded for diagnostics; recovery derives the canonical paths from the
-- registered workspace id and never trusts a path outside the workspace root.
CREATE TABLE IF NOT EXISTS harness_restore_journal (
    workspace_id  TEXT PRIMARY KEY,
    epoch         INTEGER NOT NULL,
    phase         TEXT NOT NULL,
    backup_path   TEXT NOT NULL,
    rollback_path TEXT NOT NULL,
    restore_path  TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);
