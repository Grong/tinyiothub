-- Remove organization_id column from devices table
-- Devices now belong directly to tenants via tenant_id, not through organizations
ALTER TABLE devices DROP COLUMN organization_id;

-- Drop the index if it exists
DROP INDEX IF EXISTS idx_devices_organization_id;
