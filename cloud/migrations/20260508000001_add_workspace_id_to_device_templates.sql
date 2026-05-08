-- Migration: Add workspace_id to device_templates for workspace isolation

ALTER TABLE device_templates ADD COLUMN workspace_id TEXT;

CREATE INDEX IF NOT EXISTS idx_device_templates_workspace ON device_templates(workspace_id);
