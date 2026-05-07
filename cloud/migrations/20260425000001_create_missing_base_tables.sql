-- Migration: Create missing base tables that were originally in storage-only migrations
-- Created: 2026-04-25
--
-- When storage migrations were consolidated into the cloud crate, several base
-- table creation migrations were accidentally omitted. This migration recreates
-- those tables with schemas compatible with the current codebase.
-- All CREATE statements use IF NOT EXISTS to avoid conflicts with existing tables.

-- ============================================================================
-- Agent tables
-- ============================================================================

CREATE TABLE IF NOT EXISTS agent_configs (
    agent_id TEXT PRIMARY KEY,
    config TEXT NOT NULL,
    config_hash TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS agent_tools (
    agent_id TEXT PRIMARY KEY,
    tool_overrides TEXT NOT NULL DEFAULT ('{"enabled": [], "disabled": []}'),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ============================================================================
-- Device traces
-- ============================================================================

CREATE TABLE IF NOT EXISTS device_traces (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    trace_type TEXT NOT NULL,
    level TEXT NOT NULL,
    category TEXT NOT NULL,
    title TEXT NOT NULL,
    message TEXT NOT NULL,
    details TEXT,
    source TEXT,
    user_id TEXT,
    session_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_device_traces_device_id ON device_traces(device_id);
CREATE INDEX IF NOT EXISTS idx_device_traces_trace_type ON device_traces(trace_type);
CREATE INDEX IF NOT EXISTS idx_device_traces_level ON device_traces(level);
CREATE INDEX IF NOT EXISTS idx_device_traces_category ON device_traces(category);
CREATE INDEX IF NOT EXISTS idx_device_traces_created_at ON device_traces(created_at);
CREATE INDEX IF NOT EXISTS idx_device_traces_user_id ON device_traces(user_id);
CREATE INDEX IF NOT EXISTS idx_device_traces_source ON device_traces(source);

-- ============================================================================
-- Token blacklist
-- ============================================================================

CREATE TABLE IF NOT EXISTS token_blacklist (
    id TEXT PRIMARY KEY,
    token_hash TEXT NOT NULL,
    user_id TEXT,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    reason TEXT DEFAULT 'logout'
);

CREATE INDEX IF NOT EXISTS idx_token_blacklist_token_hash ON token_blacklist(token_hash);
CREATE INDEX IF NOT EXISTS idx_token_blacklist_expires ON token_blacklist(expires_at);
