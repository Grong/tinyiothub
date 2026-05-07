-- Migration: Add workspace_id to notification tables for multi-tenant isolation
-- Created: 2026-04-29
--
-- NOTE: For fresh databases, these tables may not exist yet (base creation
-- migrations were consolidated). We use CREATE TABLE IF NOT EXISTS to ensure
-- the tables exist with the correct schema including workspace_id.
-- For existing databases, the CREATE statements are no-ops and workspace_id
-- is added via ALTER TABLE.

-- ============================================================================
-- STEP 1: notification_channels
-- ============================================================================

CREATE TABLE IF NOT EXISTS notification_channels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    channel_type TEXT NOT NULL,
    config TEXT NOT NULL DEFAULT '{}',
    is_enabled INTEGER NOT NULL DEFAULT 1,
    description TEXT,
    workspace_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_notification_channels_type ON notification_channels(channel_type);
CREATE INDEX IF NOT EXISTS idx_notification_channels_enabled ON notification_channels(is_enabled);
CREATE INDEX IF NOT EXISTS idx_notification_channels_workspace ON notification_channels(workspace_id);

-- ============================================================================
-- STEP 2: notification_rules
-- ============================================================================

CREATE TABLE IF NOT EXISTS notification_rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    event_type TEXT,
    event_subtype TEXT,
    event_level INTEGER,
    device_filter TEXT,
    notification_methods TEXT NOT NULL,
    recipients TEXT NOT NULL,
    enabled BOOLEAN DEFAULT TRUE,
    workspace_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_notification_rules_enabled ON notification_rules(enabled);
CREATE INDEX IF NOT EXISTS idx_notification_rules_type ON notification_rules(event_type, event_subtype);
CREATE INDEX IF NOT EXISTS idx_notification_rules_level ON notification_rules(event_level);
CREATE INDEX IF NOT EXISTS idx_notification_rules_workspace ON notification_rules(workspace_id);

-- ============================================================================
-- STEP 3: notification_history
-- ============================================================================

CREATE TABLE IF NOT EXISTS notification_history (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    notification_method TEXT NOT NULL,
    recipient TEXT NOT NULL,
    status TEXT NOT NULL,
    sent_at TEXT,
    error_message TEXT,
    workspace_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_notification_history_event ON notification_history(event_id);
CREATE INDEX IF NOT EXISTS idx_notification_history_rule ON notification_history(rule_id);
CREATE INDEX IF NOT EXISTS idx_notification_history_status ON notification_history(status);
CREATE INDEX IF NOT EXISTS idx_notification_history_created ON notification_history(created_at);
CREATE INDEX IF NOT EXISTS idx_notification_history_workspace ON notification_history(workspace_id);
