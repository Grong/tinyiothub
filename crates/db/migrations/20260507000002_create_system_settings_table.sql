-- Migration: Create system_settings table for configuration persistence
-- Created: 2026-05-07

CREATE TABLE IF NOT EXISTS system_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_system_settings_key ON system_settings(key);
