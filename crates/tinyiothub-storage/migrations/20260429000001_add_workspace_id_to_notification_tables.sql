-- Migration: Add workspace_id to notification tables for multi-tenant isolation
-- Created: 2026-04-29

-- STEP 1: Add workspace_id to notification_channels
ALTER TABLE notification_channels ADD COLUMN workspace_id TEXT;

-- Backfill: set workspace_id from channels that have no workspace
-- Channels without workspace_id are treated as shared/global
CREATE INDEX IF NOT EXISTS idx_notification_channels_workspace ON notification_channels(workspace_id);

-- STEP 2: Add workspace_id to notification_rules
ALTER TABLE notification_rules ADD COLUMN workspace_id TEXT;

-- Backfill: rules without workspace_id are treated as legacy/global
CREATE INDEX IF NOT EXISTS idx_notification_rules_workspace ON notification_rules(workspace_id);

-- STEP 3: Add workspace_id to notification_history
ALTER TABLE notification_history ADD COLUMN workspace_id TEXT;

CREATE INDEX IF NOT EXISTS idx_notification_history_workspace ON notification_history(workspace_id);
