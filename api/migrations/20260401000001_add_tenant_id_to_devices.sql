-- Add tenant_id to devices table and migrate data from organization_id
--
-- This migration adds tenant_id column, copies data from organization_id, and keeps
-- organization_id in place (not dropped) because SQLite cannot DROP a column
-- that is referenced by a foreign key constraint from an earlier migration.
-- The FK constraint (organization_id -> organizations.id) exists in migration
-- 20260106000002_rebuild_database_with_snake_case.sql and must be removed
-- in a follow-up migration before organization_id can be dropped.

-- Step 1: Add tenant_id column with index
ALTER TABLE devices ADD COLUMN tenant_id TEXT;
CREATE INDEX idx_devices_tenant_id ON devices(tenant_id);

-- Step 2: Migrate data from organization_id to tenant_id
UPDATE devices SET tenant_id = organization_id WHERE organization_id IS NOT NULL;

-- Note: organization_id column is NOT dropped here because SQLite DROP COLUMN
-- fails when the column is referenced by an FK constraint. A future migration
-- (after FK constraint removal) will drop organization_id.
