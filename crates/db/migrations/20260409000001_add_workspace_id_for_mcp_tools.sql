-- Migration: Add workspace_id for MCP tool isolation
-- Author: Claude Code
-- Date: 2026-04-09
-- Description:
--   - api_keys: replace tenant_id with workspace_id
--   - device_alarms: add workspace_id for isolation
--   - device_alarm_rules: add workspace_id for isolation
--   - jobs: add workspace_id for isolation
--   - Add indexes for workspace_id lookups
--
-- This migration is designed for offline execution (requires downtime window).
-- Run with: sqlite3 tinyiothub.db < migrations/20260409000001_add_workspace_id_for_mcp_tools.sql

-- ============================================================================
-- PRE-CHECK: Validate preconditions before migration
-- ============================================================================

-- Check 1: api_keys must have data (warn if empty)
-- SELECT COUNT(*) AS api_keys_count FROM api_keys;

-- Check 2: devices must all have workspace_id (critical)
-- If any device has NULL workspace_id, the alarm backfill will produce orphan alarms
-- SELECT COUNT(*) AS orphan_devices FROM devices WHERE workspace_id IS NULL;

-- Check 3: device_alarms must not have existing workspace_id (should be NULL)
-- SELECT COUNT(*) AS existing_workspace FROM device_alarms WHERE workspace_id IS NOT NULL;

-- Check 4: device_alarm_rules must not have existing workspace_id (should be NULL)
-- SELECT COUNT(*) AS existing_workspace FROM device_alarm_rules WHERE workspace_id IS NOT NULL;

-- Check 5: (jobs table replaced by cron_jobs in cron refactor; skipped)

-- Check 6: No code should reference api_keys.tenant_id after this migration
-- (Verify by grepping source code before running)

-- ============================================================================
-- STEP 1: Add workspace_id to api_keys (replace tenant_id)
-- ============================================================================

-- api_keys: rename tenant_id -> workspace_id
-- SQLite doesn't support DROP COLUMN directly, so we recreate the table
CREATE TABLE IF NOT EXISTS api_keys_new (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    prefix TEXT NOT NULL,
    permissions TEXT NOT NULL DEFAULT '["read"]',
    rate_limit INTEGER NOT NULL DEFAULT 60,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    is_revoked INTEGER NOT NULL DEFAULT 0,
    last_used_at TEXT,
    last_used_ip TEXT,
    request_count INTEGER NOT NULL DEFAULT 0,
    expires_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);

-- Copy data: tenant_id becomes workspace_id (since each key was tenant-scoped)
-- No data transformation needed for the column value itself
INSERT INTO api_keys_new
SELECT
    id,
    tenant_id AS workspace_id,  -- rename the column meaning: tenant_id -> workspace_id
    name,
    key_hash,
    prefix,
    permissions,
    rate_limit,
    is_enabled,
    is_revoked,
    last_used_at,
    last_used_ip,
    request_count,
    expires_at,
    created_at,
    updated_at
FROM api_keys;

-- Verify row count matches
-- SELECT COUNT(*) AS before_count FROM api_keys;
-- SELECT COUNT(*) AS after_count FROM api_keys_new;

-- Drop old table and rename
DROP TABLE api_keys;
ALTER TABLE api_keys_new RENAME TO api_keys;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_api_keys_workspace ON api_keys(workspace_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_prefix ON api_keys(prefix);

-- ============================================================================
-- STEP 2: Add workspace_id to device_alarms table
-- ============================================================================

ALTER TABLE device_alarms ADD COLUMN workspace_id TEXT;

-- Backfill: set workspace_id from device's workspace_id
-- Alarms that reference deleted devices or devices with NULL workspace_id will get NULL
-- These orphan alarms will be invisible to new queries (by design)
UPDATE device_alarms
SET workspace_id = (
    SELECT d.workspace_id FROM devices d WHERE d.id = device_alarms.device_id
)
WHERE workspace_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_device_alarms_workspace ON device_alarms(workspace_id);

-- ============================================================================
-- STEP 3: Add workspace_id to device_alarm_rules table
-- ============================================================================

ALTER TABLE device_alarm_rules ADD COLUMN workspace_id TEXT;

-- Backfill: if rule has device_id, use that device's workspace; else NULL
-- Rules without device_id are considered tenant-global and will get NULL workspace_id
UPDATE device_alarm_rules
SET workspace_id = (
    SELECT d.workspace_id FROM devices d WHERE d.id = device_alarm_rules.device_id
)
WHERE workspace_id IS NULL AND device_alarm_rules.device_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_device_alarm_rules_workspace ON device_alarm_rules(workspace_id);

-- ============================================================================
-- STEP 4: Add workspace_id to cron_jobs table
-- ============================================================================
-- NOTE: The old `jobs` table was replaced by `cron_jobs` in the cron refactor.
-- The cron_jobs table is created by migration 20260418000002_create_cron_tables.sql
-- which includes workspace_id directly, so no ALTER is needed here.
-- This step intentionally does nothing for fresh databases.

-- For legacy databases that still have the old jobs table:
-- ALTER TABLE jobs ADD COLUMN workspace_id TEXT;
-- UPDATE jobs SET workspace_id = (
--     SELECT d.workspace_id FROM devices d WHERE d.id = jobs.target_device_id
-- ) WHERE workspace_id IS NULL AND jobs.target_device_id IS NOT NULL;
-- CREATE INDEX IF NOT EXISTS idx_jobs_workspace ON jobs(workspace_id);

-- ============================================================================
-- POST-CHECK: Verify migration results
-- ============================================================================

-- Verify api_keys has no NULL workspace_id
-- SELECT COUNT(*) AS null_workspace_id_keys FROM api_keys WHERE workspace_id IS NULL;

-- Verify api_keys table structure
-- PRAGMA table_info(api_keys);

-- Verify indexes exist
-- SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='api_keys';
-- SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='device_alarms';
-- SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='device_alarm_rules';
