-- Seed per-thing properties and actions for all existing devices.
-- Each device gets its own property/action instances directly — no template_id dependency.
-- The thing_templates table is a creation-time blueprint; NOT queried at runtime.

-- ============================================================================
-- Step 1: If rename hasn't run, rename tables (idempotent)
-- ============================================================================
-- SQLite RENAME TO fails if table doesn't exist or target exists — use try pattern
CREATE TABLE IF NOT EXISTS thing_properties (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    name TEXT NOT NULL,
    display_name TEXT,
    data_type TEXT NOT NULL DEFAULT 'string',
    unit TEXT,
    is_read_only INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS thing_actions (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    name TEXT NOT NULL,
    display_name TEXT,
    parameters TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE
);

-- ============================================================================
-- Step 2: Seed properties for ALL existing devices
-- Properties are derived from the device's driver/capabilities, not templates.
-- ============================================================================

-- Generic properties every device gets
INSERT OR IGNORE INTO thing_properties (id, device_id, name, display_name, data_type, unit, is_read_only)
SELECT lower(hex(randomblob(16))), d.id, 'status', '在线状态', 'string', '', 1
FROM devices d WHERE NOT EXISTS (SELECT 1 FROM thing_properties p WHERE p.device_id = d.id AND p.name = 'status');

INSERT OR IGNORE INTO thing_properties (id, device_id, name, display_name, data_type, unit, is_read_only)
SELECT lower(hex(randomblob(16))), d.id, 'uptime', '运行时间', 'number', 's', 1
FROM devices d WHERE NOT EXISTS (SELECT 1 FROM thing_properties p WHERE p.device_id = d.id AND p.name = 'uptime');

INSERT OR IGNORE INTO thing_properties (id, device_id, name, display_name, data_type, unit, is_read_only)
SELECT lower(hex(randomblob(16))), d.id, 'last_heartbeat', '最后心跳', 'string', '', 1
FROM devices d WHERE NOT EXISTS (SELECT 1 FROM thing_properties p WHERE p.device_id = d.id AND p.name = 'last_heartbeat');

INSERT OR IGNORE INTO thing_properties (id, device_id, name, display_name, data_type, unit, is_read_only)
SELECT lower(hex(randomblob(16))), d.id, 'firmware_version', '固件版本', 'string', '', 1
FROM devices d WHERE NOT EXISTS (SELECT 1 FROM thing_properties p WHERE p.device_id = d.id AND p.name = 'firmware_version');

-- Camera-type devices
INSERT OR IGNORE INTO thing_properties (id, device_id, name, display_name, data_type, unit, is_read_only)
SELECT lower(hex(randomblob(16))), d.id, 'resolution', '分辨率', 'string', '', 1
FROM devices d WHERE d.device_type LIKE '%摄像%' OR d.device_type LIKE '%camera%'
  AND NOT EXISTS (SELECT 1 FROM thing_properties p WHERE p.device_id = d.id AND p.name = 'resolution');

INSERT OR IGNORE INTO thing_properties (id, device_id, name, display_name, data_type, unit, is_read_only)
SELECT lower(hex(randomblob(16))), d.id, 'frame_rate', '帧率', 'number', 'fps', 1
FROM devices d WHERE d.device_type LIKE '%摄像%' OR d.device_type LIKE '%camera%'
  AND NOT EXISTS (SELECT 1 FROM thing_properties p WHERE p.device_id = d.id AND p.name = 'frame_rate');

INSERT OR IGNORE INTO thing_properties (id, device_id, name, display_name, data_type, unit, is_read_only)
SELECT lower(hex(randomblob(16))), d.id, 'stream_url', '推流地址', 'string', '', 1
FROM devices d WHERE d.device_type LIKE '%摄像%' OR d.device_type LIKE '%camera%'
  AND NOT EXISTS (SELECT 1 FROM thing_properties p WHERE p.device_id = d.id AND p.name = 'stream_url');

-- Sensor-type devices
INSERT OR IGNORE INTO thing_properties (id, device_id, name, display_name, data_type, unit, is_read_only)
SELECT lower(hex(randomblob(16))), d.id, 'temperature', '温度', 'number', '°C', 1
FROM devices d WHERE d.device_type LIKE '%传感器%' OR d.device_type LIKE '%sensor%' OR d.device_type LIKE '%环境%'
  AND NOT EXISTS (SELECT 1 FROM thing_properties p WHERE p.device_id = d.id AND p.name = 'temperature');

INSERT OR IGNORE INTO thing_properties (id, device_id, name, display_name, data_type, unit, is_read_only)
SELECT lower(hex(randomblob(16))), d.id, 'humidity', '湿度', 'number', '%RH', 1
FROM devices d WHERE d.device_type LIKE '%传感器%' OR d.device_type LIKE '%sensor%' OR d.device_type LIKE '%环境%'
  AND NOT EXISTS (SELECT 1 FROM thing_properties p WHERE p.device_id = d.id AND p.name = 'humidity');

-- Gateway-type devices
INSERT OR IGNORE INTO thing_properties (id, device_id, name, display_name, data_type, unit, is_read_only)
SELECT lower(hex(randomblob(16))), d.id, 'connected_devices', '已连接设备数', 'number', '', 1
FROM devices d WHERE d.device_type LIKE '%网关%' OR d.device_type LIKE '%gateway%'
  AND NOT EXISTS (SELECT 1 FROM thing_properties p WHERE p.device_id = d.id AND p.name = 'connected_devices');

INSERT OR IGNORE INTO thing_properties (id, device_id, name, display_name, data_type, unit, is_read_only)
SELECT lower(hex(randomblob(16))), d.id, 'network_throughput', '网络吞吐', 'number', 'Mbps', 1
FROM devices d WHERE d.device_type LIKE '%网关%' OR d.device_type LIKE '%gateway%'
  AND NOT EXISTS (SELECT 1 FROM thing_properties p WHERE p.device_id = d.id AND p.name = 'network_throughput');

-- Power meter devices
INSERT OR IGNORE INTO thing_properties (id, device_id, name, display_name, data_type, unit, is_read_only)
SELECT lower(hex(randomblob(16))), d.id, 'voltage', '电压', 'number', 'V', 1
FROM devices d WHERE d.device_type LIKE '%电力%' OR d.device_type LIKE '%电表%' OR d.device_type LIKE '%power%'
  AND NOT EXISTS (SELECT 1 FROM thing_properties p WHERE p.device_id = d.id AND p.name = 'voltage');

INSERT OR IGNORE INTO thing_properties (id, device_id, name, display_name, data_type, unit, is_read_only)
SELECT lower(hex(randomblob(16))), d.id, 'current', '电流', 'number', 'A', 1
FROM devices d WHERE d.device_type LIKE '%电力%' OR d.device_type LIKE '%电表%' OR d.device_type LIKE '%power%'
  AND NOT EXISTS (SELECT 1 FROM thing_properties p WHERE p.device_id = d.id AND p.name = 'current');

INSERT OR IGNORE INTO thing_properties (id, device_id, name, display_name, data_type, unit, is_read_only)
SELECT lower(hex(randomblob(16))), d.id, 'power', '功率', 'number', 'kW', 1
FROM devices d WHERE d.device_type LIKE '%电力%' OR d.device_type LIKE '%电表%' OR d.device_type LIKE '%power%'
  AND NOT EXISTS (SELECT 1 FROM thing_properties p WHERE p.device_id = d.id AND p.name = 'power');

-- ============================================================================
-- Step 3: Seed actions for ALL existing devices
-- ============================================================================

-- All devices get basic actions
INSERT OR IGNORE INTO thing_actions (id, device_id, name, display_name, parameters)
SELECT lower(hex(randomblob(16))), d.id, 'reboot', '重启设备', NULL
FROM devices d WHERE NOT EXISTS (SELECT 1 FROM thing_actions a WHERE a.device_id = d.id AND a.name = 'reboot');

INSERT OR IGNORE INTO thing_actions (id, device_id, name, display_name, parameters)
SELECT lower(hex(randomblob(16))), d.id, 'update_firmware', '升级固件', '[{"name":"version","type":"string","required":true}]'
FROM devices d WHERE NOT EXISTS (SELECT 1 FROM thing_actions a WHERE a.device_id = d.id AND a.name = 'update_firmware');

-- Camera actions
INSERT OR IGNORE INTO thing_actions (id, device_id, name, display_name, parameters)
SELECT lower(hex(randomblob(16))), d.id, 'snapshot', '抓拍', NULL
FROM devices d WHERE d.device_type LIKE '%摄像%' OR d.device_type LIKE '%camera%'
  AND NOT EXISTS (SELECT 1 FROM thing_actions a WHERE a.device_id = d.id AND a.name = 'snapshot');

INSERT OR IGNORE INTO thing_actions (id, device_id, name, display_name, parameters)
SELECT lower(hex(randomblob(16))), d.id, 'start_recording', '开始录像', NULL
FROM devices d WHERE d.device_type LIKE '%摄像%' OR d.device_type LIKE '%camera%'
  AND NOT EXISTS (SELECT 1 FROM thing_actions a WHERE a.device_id = d.id AND a.name = 'start_recording');

-- Sensor/Gateway actions
INSERT OR IGNORE INTO thing_actions (id, device_id, name, display_name, parameters)
SELECT lower(hex(randomblob(16))), d.id, 'set_report_interval', '设置上报间隔', '[{"name":"interval","type":"number","required":true,"unit":"s"}]'
FROM devices d
WHERE (d.device_type LIKE '%传感器%' OR d.device_type LIKE '%sensor%' OR d.device_type LIKE '%网关%' OR d.device_type LIKE '%gateway%' OR d.device_type LIKE '%环境%')
  AND NOT EXISTS (SELECT 1 FROM thing_actions a WHERE a.device_id = d.id AND a.name = 'set_report_interval');
