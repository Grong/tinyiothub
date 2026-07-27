-- Thing Model Cleanup Migration (eng-review 2026-07-27, task T3/T12)
--
-- Repairs the thing-model data path found in engineering review:
--   1. thing_properties / thing_actions rebuilt with the FULL original schema.
--      00003 created stripped tables missing description / min_value /
--      max_value / default_value (properties), description / updated_at
--      (actions), and UNIQUE(device_id, name).
--   2. Synthetic seed rows inserted by 00003 are deleted (exact
--      name + display_name tuples). These invented capabilities
--      ('reboot', 'temperature', ...) for every device regardless of hardware.
--   3. device_alarm_rules.property_id FK repointed device_properties →
--      thing_properties (IDs preserved, so existing rules keep resolving).
--   4. device_event_triggers dropped (deprecated; superseded by
--      rule_type='event' alarm rules, zero code references).
--   5. resources.parse_status dropped (knowledge-parse pipeline deleted with
--      the workspace graph; the column has no writer).
--
-- Real-data copy from the pre-branch device_properties / device_commands
-- tables cannot be expressed in plain SQL (the tables may or may not exist,
-- depending on which schema the deployment started from). It runs in Rust:
-- persistence::migrations::repair_thing_model_data(), invoked by
-- run_migrations() right after the Migrator. That step copies rows
-- PRESERVING IDs (INSERT OR IGNORE) and then drops the old tables.
--
-- Note: 00003 itself is intentionally NOT edited — it is already applied
-- with a recorded checksum on live databases; this migration neutralizes
-- its seed data on every environment, whether or not 00003 ran.

PRAGMA defer_foreign_keys = ON;

-- ============================================================================
-- 1a. thing_properties rebuild (full schema; copy keeps existing rows + IDs)
-- ============================================================================
-- Column subset copied is the intersection guaranteed by the 00003 stripped
-- schema; metadata columns the stripped table never had are filled with NULL.

CREATE TABLE thing_properties_new (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    name TEXT NOT NULL,
    display_name TEXT,
    description TEXT,
    data_type TEXT NOT NULL DEFAULT 'string',
    unit TEXT,
    min_value REAL,
    max_value REAL,
    default_value TEXT,
    is_read_only INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE,
    UNIQUE(device_id, name)
);

INSERT OR IGNORE INTO thing_properties_new
    (id, device_id, name, display_name, description, data_type, unit,
     min_value, max_value, default_value, is_read_only, created_at, updated_at)
SELECT id, device_id, name, display_name,
       NULL, data_type, unit,
       NULL, NULL, NULL, is_read_only, created_at, updated_at
FROM thing_properties;

DROP TABLE thing_properties;
ALTER TABLE thing_properties_new RENAME TO thing_properties;

CREATE INDEX IF NOT EXISTS idx_thing_properties_device_id ON thing_properties(device_id);

-- ============================================================================
-- 1b. thing_actions rebuild (full schema)
-- ============================================================================

CREATE TABLE thing_actions_new (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    name TEXT NOT NULL,
    display_name TEXT,
    description TEXT,
    parameters TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE,
    UNIQUE(device_id, name)
);

INSERT OR IGNORE INTO thing_actions_new
    (id, device_id, name, display_name, description, parameters, created_at, updated_at)
SELECT id, device_id, name, display_name,
       NULL, parameters, created_at, created_at
FROM thing_actions;

DROP TABLE thing_actions;
ALTER TABLE thing_actions_new RENAME TO thing_actions;

CREATE INDEX IF NOT EXISTS idx_thing_actions_device_id ON thing_actions(device_id);

-- ============================================================================
-- 2. Delete synthetic seed rows inserted by 00003
-- ============================================================================
-- Exact (name, display_name) tuples from the seed INSERTs. Runs AFTER the
-- rebuild but BEFORE the Rust copy of real device_properties data, so real
-- rows can never match these tuples on upgrade-from-main deployments.

DELETE FROM thing_properties
WHERE (name, display_name) IN (VALUES
    ('status', '在线状态'),
    ('uptime', '运行时间'),
    ('last_heartbeat', '最后心跳'),
    ('firmware_version', '固件版本'),
    ('resolution', '分辨率'),
    ('frame_rate', '帧率'),
    ('stream_url', '推流地址'),
    ('temperature', '温度'),
    ('humidity', '湿度'),
    ('connected_devices', '已连接设备数'),
    ('network_throughput', '网络吞吐'),
    ('voltage', '电压'),
    ('current', '电流'),
    ('power', '功率')
);

DELETE FROM thing_actions
WHERE (name, display_name) IN (VALUES
    ('reboot', '重启设备'),
    ('update_firmware', '升级固件'),
    ('snapshot', '抓拍'),
    ('start_recording', '开始录像'),
    ('set_report_interval', '设置上报间隔')
);

-- ============================================================================
-- 3. device_alarm_rules rebuild: property_id FK → thing_properties
-- ============================================================================

CREATE TABLE device_alarm_rules_new (
    id TEXT PRIMARY KEY,
    device_id TEXT,
    property_id TEXT,
    rule_name TEXT NOT NULL,
    rule_type TEXT NOT NULL CHECK (rule_type IN ('threshold', 'range', 'change', 'offline', 'event')),
    condition_config TEXT NOT NULL,
    alarm_level TEXT NOT NULL CHECK (alarm_level IN ('info', 'warning', 'error', 'critical')),
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    description TEXT,
    workspace_id TEXT,
    notification_config TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE,
    FOREIGN KEY (property_id) REFERENCES thing_properties(id) ON DELETE CASCADE,
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL
);

INSERT INTO device_alarm_rules_new
    (id, device_id, property_id, rule_name, rule_type, condition_config,
     alarm_level, is_enabled, description, workspace_id, notification_config,
     created_by, created_at, updated_at)
SELECT id, device_id, property_id, rule_name, rule_type, condition_config,
       alarm_level, is_enabled, description, workspace_id,
       COALESCE(notification_config, NULL),
       created_by, created_at, updated_at
FROM device_alarm_rules;

DROP TABLE device_alarm_rules;
ALTER TABLE device_alarm_rules_new RENAME TO device_alarm_rules;

CREATE INDEX IF NOT EXISTS idx_device_alarm_rules_device_id ON device_alarm_rules(device_id);
CREATE INDEX IF NOT EXISTS idx_device_alarm_rules_is_enabled ON device_alarm_rules(is_enabled);

-- ============================================================================
-- 3b. device_alarms rebuild: property_id FK → thing_properties
-- ============================================================================
-- The original device_alarms references device_properties(id) — dropping the
-- old table without this rebuild leaves a dangling FK parent, and SQLite then
-- refuses ANY delete from devices ("no such table: main.device_properties",
-- found via gateway pairing rollback test).

CREATE TABLE device_alarms_new (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    property_id TEXT,
    rule_id TEXT,
    alarm_level TEXT NOT NULL CHECK (alarm_level IN ('info', 'warning', 'error', 'critical')),
    alarm_message TEXT NOT NULL,
    alarm_value TEXT,
    threshold_value TEXT,
    alarm_time TEXT NOT NULL,
    is_acknowledged BOOLEAN NOT NULL DEFAULT false,
    acknowledged_by TEXT,
    acknowledged_at TEXT,
    acknowledged_note TEXT,
    is_resolved BOOLEAN NOT NULL DEFAULT false,
    resolved_at TEXT,
    resolved_by TEXT,
    resolved_note TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    workspace_id TEXT,
    resolution_type TEXT,
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE,
    FOREIGN KEY (property_id) REFERENCES thing_properties(id) ON DELETE SET NULL,
    FOREIGN KEY (rule_id) REFERENCES device_alarm_rules(id) ON DELETE SET NULL,
    FOREIGN KEY (acknowledged_by) REFERENCES users(id) ON DELETE SET NULL,
    FOREIGN KEY (resolved_by) REFERENCES users(id) ON DELETE SET NULL
);

INSERT INTO device_alarms_new
    (id, device_id, property_id, rule_id, alarm_level, alarm_message,
     alarm_value, threshold_value, alarm_time, is_acknowledged,
     acknowledged_by, acknowledged_at, acknowledged_note, is_resolved,
     resolved_at, resolved_by, resolved_note, created_at, workspace_id,
     resolution_type)
SELECT id, device_id, property_id, rule_id, alarm_level, alarm_message,
       alarm_value, threshold_value, alarm_time, is_acknowledged,
       acknowledged_by, acknowledged_at, acknowledged_note, is_resolved,
       resolved_at, resolved_by, resolved_note, created_at, workspace_id,
       resolution_type
FROM device_alarms;

DROP TABLE device_alarms;
ALTER TABLE device_alarms_new RENAME TO device_alarms;

CREATE INDEX IF NOT EXISTS idx_device_alarms_device_id ON device_alarms(device_id);
CREATE INDEX IF NOT EXISTS idx_device_alarms_alarm_level ON device_alarms(alarm_level);
CREATE INDEX IF NOT EXISTS idx_device_alarms_alarm_time ON device_alarms(alarm_time);
CREATE INDEX IF NOT EXISTS idx_device_alarms_is_acknowledged ON device_alarms(is_acknowledged);
CREATE INDEX IF NOT EXISTS idx_device_alarms_is_resolved ON device_alarms(is_resolved);

-- ============================================================================
-- 4. Drop device_event_triggers (deprecated, zero code references)
-- ============================================================================

DROP TABLE IF EXISTS device_event_triggers;

-- ============================================================================
-- 5. Drop resources.parse_status (dead column, no writer since graph teardown)
-- ============================================================================

ALTER TABLE resources DROP COLUMN parse_status;

-- ============================================================================
-- 6. Referential integrity check (rows enforced Rust-side by
--    migrations::enforce_foreign_key_integrity after the Migrator runs)
-- ============================================================================
PRAGMA foreign_key_check;
