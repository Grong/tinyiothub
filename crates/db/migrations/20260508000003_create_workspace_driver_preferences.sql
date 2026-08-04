-- TODO: This table is reserved for future workspace-level driver version
-- pinning and auto-update preferences. No Rust code references it yet.
-- See issue #43.
CREATE TABLE IF NOT EXISTS workspace_driver_preferences (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    driver_name TEXT NOT NULL,
    preferred_version TEXT NOT NULL,
    auto_update INTEGER DEFAULT 0,
    UNIQUE(workspace_id, driver_name)
);

CREATE INDEX IF NOT EXISTS idx_workspace_driver_prefs_workspace ON workspace_driver_preferences(workspace_id);
