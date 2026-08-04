-- Migration: Add workspace_id to notification tables for multi-tenant isolation
-- Created: 2026-04-29
--
-- Uses ALTER TABLE ADD COLUMN so existing tables get the new column.
-- For fresh databases the base migrations create the tables without
-- workspace_id, then this migration adds it.

-- notification_channels
ALTER TABLE notification_channels ADD COLUMN workspace_id TEXT;
CREATE INDEX IF NOT EXISTS idx_notification_channels_workspace ON notification_channels(workspace_id);

-- notification_rules
ALTER TABLE notification_rules ADD COLUMN workspace_id TEXT;
CREATE INDEX IF NOT EXISTS idx_notification_rules_workspace ON notification_rules(workspace_id);

-- notification_history
ALTER TABLE notification_history ADD COLUMN workspace_id TEXT;
CREATE INDEX IF NOT EXISTS idx_notification_history_workspace ON notification_history(workspace_id);
