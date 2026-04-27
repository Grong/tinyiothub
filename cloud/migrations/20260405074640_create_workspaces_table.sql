-- Create workspaces table
CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    tenant_id TEXT NOT NULL,
    agent_id TEXT,
    agent_config TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Index for fast tenant-scoped queries
CREATE INDEX IF NOT EXISTS idx_workspaces_tenant ON workspaces(tenant_id);
CREATE INDEX IF NOT EXISTS idx_workspaces_agent ON workspaces(agent_id);

-- Alter devices table to add workspace_id
-- Device can only belong to one workspace at a time (null = unassigned)
ALTER TABLE devices ADD COLUMN workspace_id TEXT REFERENCES workspaces(id) ON DELETE SET NULL;

-- Index for finding devices by workspace
CREATE INDEX IF NOT EXISTS idx_devices_workspace ON devices(workspace_id);

-- Unique constraint: device can only be in one workspace at a time
-- This prevents race conditions in assign_device
-- Note: devices with workspace_id=NULL are allowed multiple (unassigned devices)
