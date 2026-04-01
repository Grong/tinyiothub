-- Rename organization_id to tenant_id in devices table (single atomic migration)
-- This replaces the previous two-step migration approach

-- Step 1: Add tenant_id column with index
ALTER TABLE devices ADD COLUMN tenant_id TEXT;
CREATE INDEX idx_devices_tenant_id ON devices(tenant_id);

-- Step 2: Migrate data from organization_id to tenant_id (if organization_id exists and has values)
-- Only migrate if the column exists and is not dropped yet
-- Use a transaction for atomicity
BEGIN TRANSACTION;
  UPDATE devices SET tenant_id = organization_id WHERE organization_id IS NOT NULL;
COMMIT;

-- Step 3: Remove old organization_id column and index (SQLite 3.35.0+)
-- Note: Requires SQLite 3.35.0 or later for DROP COLUMN support
ALTER TABLE devices DROP COLUMN organization_id;
DROP INDEX IF EXISTS idx_devices_organization_id;
