-- Relax device_alarm_rules constraints to support:
-- - Global rules (no device_id)
-- - Rules that apply to all properties (no property_id)
-- - Rules created by system/API (no created_by user)

-- 1. Recreate device_alarm_rules with relaxed constraints
-- SQLite doesn't support ALTER COLUMN, so we recreate the table

CREATE TABLE IF NOT EXISTS device_alarm_rules_new (
    id TEXT PRIMARY KEY,
    device_id TEXT,                           -- NULL = global rule
    property_id TEXT,                          -- NULL = all properties
    rule_name TEXT NOT NULL,
    rule_type TEXT NOT NULL,
    condition_config TEXT NOT NULL,
    alarm_level TEXT NOT NULL CHECK (alarm_level IN ('info', 'warning', 'error', 'critical')),
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    description TEXT,
    workspace_id TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE,
    FOREIGN KEY (property_id) REFERENCES device_properties(id) ON DELETE CASCADE,
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL
);

-- Copy existing data
INSERT INTO device_alarm_rules_new SELECT * FROM device_alarm_rules;

-- Drop old table and rename
DROP TABLE device_alarm_rules;
ALTER TABLE device_alarm_rules_new RENAME TO device_alarm_rules;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_device_alarm_rules_device_id ON device_alarm_rules(device_id);
CREATE INDEX IF NOT EXISTS idx_device_alarm_rules_is_enabled ON device_alarm_rules(is_enabled);
