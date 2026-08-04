CREATE TABLE IF NOT EXISTS driver_installations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    driver_name TEXT NOT NULL,
    version TEXT NOT NULL,
    file_path TEXT NOT NULL,
    checksum TEXT NOT NULL,
    protocol_type TEXT,
    installed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(workspace_id, driver_name, version)
);

CREATE INDEX IF NOT EXISTS idx_driver_installations_workspace ON driver_installations(workspace_id);
CREATE INDEX IF NOT EXISTS idx_driver_installations_driver ON driver_installations(driver_name);
