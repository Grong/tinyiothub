-- Create device_templates table
CREATE TABLE IF NOT EXISTS device_templates (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    display_name TEXT NOT NULL,
    description TEXT,
    version TEXT NOT NULL,
    author TEXT,
    category TEXT NOT NULL,
    manufacturer TEXT,
    device_type TEXT NOT NULL,
    protocol_type TEXT,
    driver_name TEXT,
    tags TEXT NOT NULL DEFAULT '[]',
    device_info TEXT NOT NULL DEFAULT '{}',
    properties TEXT NOT NULL DEFAULT '[]',
    commands TEXT NOT NULL DEFAULT '[]',
    is_builtin INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_device_templates_name ON device_templates(name);
CREATE INDEX IF NOT EXISTS idx_device_templates_category ON device_templates(category);
CREATE INDEX IF NOT EXISTS idx_device_templates_is_active ON device_templates(is_active);
CREATE INDEX IF NOT EXISTS idx_device_templates_is_builtin ON device_templates(is_builtin);
