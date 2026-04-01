-- Add tenant_id column to devices table
ALTER TABLE devices ADD COLUMN tenant_id TEXT;

-- Create index for tenant-based queries
CREATE INDEX idx_devices_tenant_id ON devices(tenant_id);
