CREATE TABLE workspace_resources (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    resource_type TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    file_path TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    metadata TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_resources_workspace ON workspace_resources(workspace_id);
CREATE INDEX idx_resources_type ON workspace_resources(resource_type);
CREATE INDEX idx_resources_name ON workspace_resources(name);
