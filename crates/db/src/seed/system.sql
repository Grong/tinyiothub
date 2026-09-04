-- seed_system: production-required seed rows (extracted from the pre-baseline
-- migration chain; see crates/db/src/seed.rs for the provenance table).
-- Every statement is idempotent (INSERT OR IGNORE / WHERE NOT EXISTS).

-- ── subscription_plans (20260313000001) ─────────────────────────────────────
-- tenants.plan_id FK target; must exist before any tenant row.
INSERT OR IGNORE INTO subscription_plans (id, name, display_name, description, thing_limit, api_call_limit, storage_mb, user_limit, price_monthly, price_yearly, features, sort_order) VALUES
    ('plan_free', 'free', '免费版', '适合个人测试', 10, 1000, 100, 2, 0, 0, '{"thing_group": false, "webhook": true, "sms": false, "email": false, "api_access": true, "custom_brand": false}', 1),
    ('plan_basic', 'basic', '基础版', '适合小型项目', 100, 10000, 1024, 5, 99, 990, '{"thing_group": true, "webhook": true, "sms": true, "email": true, "api_access": true, "custom_brand": false}', 2),
    ('plan_pro', 'pro', '专业版', '适合中大型项目', 1000, 100000, 10240, 20, 399, 3990, '{"thing_group": true, "webhook": true, "sms": true, "email": true, "api_access": true, "custom_brand": true}', 3),
    ('plan_enterprise', 'enterprise', '企业版', '适合大型企业', 0, 0, 0, 0, 0, 0, '{"thing_group": true, "webhook": true, "sms": true, "email": true, "api_access": true, "custom_brand": true, "dedicated_support": true, "sla": true}', 4);

-- ── template_categories (20260108000001) ────────────────────────────────────
-- thing_templates.category FK target.
INSERT OR IGNORE INTO template_categories (name, display_name, description, sort_order, is_active, created_at) VALUES
('sensors', '{"zh": "传感器", "en": "Sensors"}', '{"zh": "各类传感器设备模板", "en": "Various sensor device templates"}', 1, 1, datetime('now')),
('cameras', '{"zh": "摄像头", "en": "Cameras"}', '{"zh": "监控摄像头设备模板", "en": "Surveillance camera device templates"}', 2, 1, datetime('now')),
('controllers', '{"zh": "控制器", "en": "Controllers"}', '{"zh": "各类控制器设备模板", "en": "Various controller device templates"}', 3, 1, datetime('now')),
('robots', '{"zh": "机器人", "en": "Robots"}', '{"zh": "工业机器人设备模板", "en": "Industrial robot device templates"}', 4, 1, datetime('now')),
('gateways', '{"zh": "网关", "en": "Gateways"}', '{"zh": "通信网关设备模板", "en": "Communication gateway device templates"}', 5, 1, datetime('now')),
('meters', '{"zh": "仪表", "en": "Meters"}', '{"zh": "各类仪表设备模板", "en": "Various meter device templates"}', 6, 1, datetime('now')),
('scenes', '{"zh": "场景包", "en": "Scene Packs"}', '{"zh": "空间组合模板：园区/楼宇/楼层", "en": "Spatial composition templates"}', 7, 1, datetime('now'));

-- ── admin user (20260106000002 + 20260329000001) ────────────────────────────
-- Password hash carries the FIX_ME marker; ensure_default_admin_user (cloud
-- startup) replaces it with a real hash on first boot.
INSERT OR IGNORE INTO users (id, username, password_hash, display_name, is_enabled) VALUES
('admin-user-001', 'admin', 'FIX_ME_admin_hash', '系统管理员', true);

-- ── RBAC baseline (20260106000002 + 20260112000001) ─────────────────────────
INSERT OR IGNORE INTO roles (id, name, description, is_administrator) VALUES
('role-admin', '系统管理员', '拥有系统所有权限', true),
('role-operator', '操作员', '设备操作和监控权限', false),
('role-viewer', '查看者', '只读权限', false);

INSERT OR IGNORE INTO user_roles (id, user_id, role_id) VALUES
('user-role-001', 'admin-user-001', 'role-admin');

INSERT OR IGNORE INTO permissions (id, name, description, resource_type, action) VALUES
('perm-thing-read', 'thing:read', '查看设备信息', 'thing', 'read'),
('perm-thing-write', 'thing:write', '修改设备信息', 'thing', 'write'),
('perm-thing-delete', 'thing:delete', '删除设备', 'thing', 'delete'),
('perm-thing-admin', 'thing:admin', '设备管理权限', 'thing', 'admin'),
('perm-user-read', 'user:read', '查看用户信息', 'user', 'read'),
('perm-user-write', 'user:write', '修改用户信息', 'user', 'write'),
('perm-user-delete', 'user:delete', '删除用户', 'user', 'delete'),
('perm-user-admin', 'user:admin', '用户管理权限', 'user', 'admin'),
('perm-system-admin', 'system:admin', '系统管理权限', 'system', 'admin'),
('perm-event-read', 'event:read', '查看事件信息', 'event', 'read'),
('perm-event-create', 'event:create', '创建事件', 'event', 'create'),
('perm-event-update', 'event:update', '修改事件信息', 'event', 'update'),
('perm-event-delete', 'event:delete', '删除事件', 'event', 'delete'),
('perm-event-admin', 'event:admin', '事件管理权限', 'event', 'admin'),
('perm-event-audit', 'event:audit', '查看事件审计日志', 'event', 'audit');

INSERT OR IGNORE INTO role_permissions (id, role_id, permission_id) VALUES
('role-perm-001', 'role-admin', 'perm-thing-admin'),
('role-perm-002', 'role-admin', 'perm-user-admin'),
('role-perm-003', 'role-admin', 'perm-system-admin'),
('role-perm-004', 'role-operator', 'perm-thing-read'),
('role-perm-005', 'role-operator', 'perm-thing-write'),
('role-perm-006', 'role-viewer', 'perm-thing-read'),
('role-perm-007', 'role-viewer', 'perm-user-read'),
('role-perm-event-001', 'role-admin', 'perm-event-admin'),
('role-perm-event-002', 'role-admin', 'perm-event-audit'),
('role-perm-event-003', 'role-operator', 'perm-event-read'),
('role-perm-event-004', 'role-operator', 'perm-event-create'),
('role-perm-event-005', 'role-operator', 'perm-event-update'),
('role-perm-event-006', 'role-viewer', 'perm-event-read');

-- ── default tenant + tenant membership + workspace (20260407000001) ─────────
-- Guard semantics preserved: only created when NO tenant exists yet.
INSERT INTO tenants (
    id, name, slug, status, plan_id, subscription_status,
    trial_expires_at, billing_email, billing_contact, timezone, locale,
    custom_logo, custom_theme, created_at, updated_at
)
SELECT
    'tenant-default-001',
    'Default Organization',
    'default',
    'active',
    'plan_free',
    'active',
    NULL,
    'admin@tinyiothub.local',
    NULL,
    'UTC',
    'zh-CN',
    NULL,
    NULL,
    datetime('now'),
    datetime('now')
WHERE NOT EXISTS (SELECT 1 FROM tenants LIMIT 1);

INSERT INTO tenant_users (
    id, tenant_id, user_id, role, invitation_status,
    joined_at, created_at, updated_at
)
SELECT
    'tu-admin-default-001',
    'tenant-default-001',
    u.id,
    'owner',
    'accepted',
    datetime('now'),
    datetime('now'),
    datetime('now')
FROM users u
WHERE u.username = 'admin'
  AND NOT EXISTS (
    SELECT 1 FROM tenant_users tu WHERE tu.user_id = u.id
  );

INSERT INTO workspaces (
    id, name, description, tenant_id, agent_id, agent_config,
    created_at, updated_at
)
SELECT
    'ws-default-001',
    '默认工作空间',
    '系统自动创建的默认工作空间',
    'tenant-default-001',
    NULL,
    NULL,
    datetime('now'),
    datetime('now')
WHERE NOT EXISTS (
    SELECT 1 FROM workspaces WHERE tenant_id = 'tenant-default-001'
);

-- ── default notification rules (20260111000001) ─────────────────────────────
INSERT OR IGNORE INTO notification_rules (
    id, name, description, event_level, notification_methods, recipients, enabled
) VALUES (
    'default-critical-events',
    '严重事件通知',
    '所有严重级别事件的默认通知规则',
    5,
    '["websocket"]',
    '["admin"]',
    1
);

INSERT OR IGNORE INTO notification_rules (
    id, name, description, event_level, notification_methods, recipients, enabled
) VALUES (
    'default-error-events',
    '错误事件通知',
    '所有错误级别事件的默认通知规则',
    4,
    '["websocket"]',
    '["admin"]',
    1
);

INSERT OR IGNORE INTO notification_rules (
    id, name, description, event_type, event_subtype, notification_methods, recipients, enabled
) VALUES (
    'device-connection-events',
    '设备连接事件通知',
    '设备连接状态变化通知',
    'device',
    'connection',
    '["websocket"]',
    '["admin", "operator"]',
    1
);

-- ── event security defaults (20260112000001) ────────────────────────────────
INSERT OR IGNORE INTO event_security_settings (id, event_type, min_role_level, require_encryption, audit_level) VALUES
('sec-001', 'system.user_auth', 1, true, 'detailed'),
('sec-002', 'system.user_operation', 1, false, 'normal'),
('sec-003', 'system.system_config', 2, true, 'detailed'),
('sec-004', 'system.system_error', 1, false, 'normal'),
('sec-005', 'device.connection', 1, false, 'basic'),
('sec-006', 'device.property', 1, false, 'basic'),
('sec-007', 'device.command', 2, false, 'normal'),
('sec-008', 'device.business', 1, false, 'basic');

-- ── event performance thresholds (20260113000001) ───────────────────────────
INSERT OR IGNORE INTO event_performance_alerts (
    id, alert_type, severity, message, current_value, threshold_value, resolved
) VALUES
    ('threshold_processing_time', 'configuration', 'info', 'Max processing time threshold: 100ms', 100.0, 100.0, 1),
    ('threshold_queue_size', 'configuration', 'info', 'Max queue size threshold: 1000', 1000.0, 1000.0, 1),
    ('threshold_error_rate', 'configuration', 'info', 'Max error rate threshold: 1%', 0.01, 0.01, 1),
    ('threshold_memory_usage', 'configuration', 'info', 'Max memory usage threshold: 80%', 80.0, 80.0, 1);

-- ── social login provider row (20260314000001) ──────────────────────────────
INSERT OR IGNORE INTO social_configs (provider, is_enabled) VALUES ('wechat', 0);

-- ── event retention cron job (20260727000003) ───────────────────────────────
INSERT INTO cron_jobs (
    id, workspace_id, name, description, job_type, cron_expression, config,
    timeout_seconds, max_retries, is_enabled, created_by, created_at, updated_at
)
SELECT
    'sys-event-retention', 'system', 'Events 保留清理',
    'Delete occurrence-type events (is_status=0) older than retention_days; status rows are never time-purged.',
    'event_retention', '0 17 3 * * *', '{"retention_days": 90}',
    300, 3, 1, NULL, datetime('now'), datetime('now')
WHERE NOT EXISTS (SELECT 1 FROM cron_jobs WHERE id = 'sys-event-retention');

-- ── builtin thing templates ─────────────────────────────────────────────────
-- 20260108000001 (5, dash-style ids) + 20260516044444 (8, underscore-style
-- ids), mapped device_templates.commands → thing_templates.actions,
-- thing_type='device', events='[]' (per the 20260723000001 rebuild mapping).
INSERT OR IGNORE INTO thing_templates (
    id, name, display_name, description, version, author, category, manufacturer,
    thing_type, protocol_type, driver_name, tags, device_info, properties, actions,
    events, default_knowledge, is_builtin, is_active, workspace_id, created_at, updated_at
) VALUES
('builtin-temperature-sensor', 'temperature_sensor', '{"zh": "温度传感器", "en": "Temperature Sensor"}', '{"zh": "标准温度传感器设备模板，支持温度监测和报警配置", "en": "Standard temperature sensor device template with temperature monitoring and alarm configuration"}', '1.0.0', 'System', 'sensors', NULL, 'device', 'modbus', 'modbus_rtu', '["sensor", "temperature", "monitoring"]', '{"default_name_pattern": "temp_sensor_{index}", "default_display_name_pattern": "温度传感器 {index}", "default_description": {"zh": "温度监测传感器", "en": "Temperature monitoring sensor"}, "required_fields": ["name", "address"]}', '[
        {
            "name": "temperature",
            "display_name": {"zh": "温度", "en": "Temperature"},
            "description": {"zh": "当前环境温度", "en": "Current ambient temperature"},
            "data_type": "number",
            "unit": "°C",
            "min_value": -50.0,
            "max_value": 200.0,
            "default_value": "25.0",
            "is_read_only": true,
            "is_required": true
        },
        {
            "name": "alarm_high_temp",
            "display_name": {"zh": "高温报警阈值", "en": "High Temperature Alarm Threshold"},
            "description": {"zh": "温度超过此值时触发报警", "en": "Trigger alarm when temperature exceeds this value"},
            "data_type": "number",
            "unit": "°C",
            "min_value": 0.0,
            "max_value": 200.0,
            "default_value": "80.0",
            "is_read_only": false,
            "is_required": false
        },
        {
            "name": "alarm_low_temp",
            "display_name": {"zh": "低温报警阈值", "en": "Low Temperature Alarm Threshold"},
            "description": {"zh": "温度低于此值时触发报警", "en": "Trigger alarm when temperature below this value"},
            "data_type": "number",
            "unit": "°C",
            "min_value": -50.0,
            "max_value": 100.0,
            "default_value": "10.0",
            "is_read_only": false,
            "is_required": false
        },
        {
            "name": "sampling_interval",
            "display_name": {"zh": "采样间隔", "en": "Sampling Interval"},
            "description": {"zh": "数据采样时间间隔", "en": "Data sampling time interval"},
            "data_type": "number",
            "unit": "秒",
            "min_value": 1.0,
            "max_value": 3600.0,
            "default_value": "60.0",
            "is_read_only": false,
            "is_required": false
        }
    ]', '[
        {
            "name": "read_temperature",
            "display_name": {"zh": "读取温度", "en": "Read Temperature"},
            "description": {"zh": "读取当前温度值", "en": "Read current temperature value"},
            "parameters": "{}",
            "is_required": true
        },
        {
            "name": "set_alarm_thresholds",
            "display_name": {"zh": "设置报警阈值", "en": "Set Alarm Thresholds"},
            "description": {"zh": "设置高低温报警阈值", "en": "Set high and low temperature alarm thresholds"},
            "parameters": "{\"high_temp\": 80, \"low_temp\": 10}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"high_temp\": {\"type\": \"number\", \"minimum\": 0, \"maximum\": 200}, \"low_temp\": {\"type\": \"number\", \"minimum\": -50, \"maximum\": 100}}, \"required\": [\"high_temp\", \"low_temp\"]}",
            "is_required": false
        },
        {
            "name": "calibrate_sensor",
            "display_name": {"zh": "校准传感器", "en": "Calibrate Sensor"},
            "description": {"zh": "执行传感器校准程序", "en": "Execute sensor calibration procedure"},
            "parameters": "{\"reference_temp\": 25}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"reference_temp\": {\"type\": \"number\", \"minimum\": -50, \"maximum\": 200}}, \"required\": [\"reference_temp\"]}",
            "is_required": false
        }
    ]', '[]', NULL, 1, 1, NULL, datetime('now'), datetime('now')),
('builtin-onvif-camera', 'onvif_camera', '{"zh": "ONVIF摄像头", "en": "ONVIF Camera"}', '{"zh": "标准ONVIF协议摄像头设备模板，支持视频流和PTZ控制", "en": "Standard ONVIF protocol camera device template with video streaming and PTZ control"}', '1.0.0', 'System', 'cameras', NULL, 'device', 'onvif', 'onvif', '["camera", "onvif", "surveillance", "ptz"]', '{"default_name_pattern": "camera_{index}", "default_display_name_pattern": "摄像头 {index}", "default_description": {"zh": "ONVIF网络摄像头", "en": "ONVIF Network Camera"}, "required_fields": ["name", "address"]}', '[
        {
            "name": "resolution",
            "display_name": {"zh": "分辨率", "en": "Resolution"},
            "description": {"zh": "视频分辨率设置", "en": "Video resolution setting"},
            "data_type": "string",
            "default_value": "1920x1080",
            "is_read_only": false,
            "is_required": true
        },
        {
            "name": "frame_rate",
            "display_name": {"zh": "帧率", "en": "Frame Rate"},
            "description": {"zh": "视频帧率设置", "en": "Video frame rate setting"},
            "data_type": "number",
            "unit": "fps",
            "min_value": 1.0,
            "max_value": 60.0,
            "default_value": "30.0",
            "is_read_only": false,
            "is_required": true
        },
        {
            "name": "pan_angle",
            "display_name": {"zh": "水平角度", "en": "Pan Angle"},
            "description": {"zh": "摄像头水平旋转角度", "en": "Camera horizontal rotation angle"},
            "data_type": "number",
            "unit": "度",
            "min_value": -180.0,
            "max_value": 180.0,
            "default_value": "0.0",
            "is_read_only": false,
            "is_required": false
        },
        {
            "name": "tilt_angle",
            "display_name": {"zh": "垂直角度", "en": "Tilt Angle"},
            "description": {"zh": "摄像头垂直旋转角度", "en": "Camera vertical rotation angle"},
            "data_type": "number",
            "unit": "度",
            "min_value": -90.0,
            "max_value": 90.0,
            "default_value": "0.0",
            "is_read_only": false,
            "is_required": false
        },
        {
            "name": "zoom_level",
            "display_name": {"zh": "变焦级别", "en": "Zoom Level"},
            "description": {"zh": "摄像头变焦倍数", "en": "Camera zoom magnification"},
            "data_type": "number",
            "unit": "x",
            "min_value": 1.0,
            "max_value": 20.0,
            "default_value": "1.0",
            "is_read_only": false,
            "is_required": false
        }
    ]', '[
        {
            "name": "get_snapshot",
            "display_name": {"zh": "获取快照", "en": "Get Snapshot"},
            "description": {"zh": "获取当前视频快照", "en": "Get current video snapshot"},
            "parameters": "{}",
            "is_required": true
        },
        {
            "name": "pan_tilt",
            "display_name": {"zh": "云台控制", "en": "Pan Tilt Control"},
            "description": {"zh": "控制摄像头水平和垂直旋转", "en": "Control camera horizontal and vertical rotation"},
            "parameters": "{\"pan_angle\": 0, \"tilt_angle\": 0}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"pan_angle\": {\"type\": \"number\", \"minimum\": -180, \"maximum\": 180}, \"tilt_angle\": {\"type\": \"number\", \"minimum\": -90, \"maximum\": 90}}, \"required\": [\"pan_angle\", \"tilt_angle\"]}",
            "is_required": false
        },
        {
            "name": "set_zoom",
            "display_name": {"zh": "设置变焦", "en": "Set Zoom"},
            "description": {"zh": "调整摄像头变焦级别", "en": "Adjust camera zoom level"},
            "parameters": "{\"zoom_level\": 1}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"zoom_level\": {\"type\": \"number\", \"minimum\": 1, \"maximum\": 20}}, \"required\": [\"zoom_level\"]}",
            "is_required": false
        },
        {
            "name": "start_recording",
            "display_name": {"zh": "开始录制", "en": "Start Recording"},
            "description": {"zh": "开始视频录制", "en": "Start video recording"},
            "parameters": "{\"duration_minutes\": 60}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"duration_minutes\": {\"type\": \"number\", \"minimum\": 1, \"maximum\": 1440}}, \"required\": [\"duration_minutes\"]}",
            "is_required": false
        },
        {
            "name": "stop_recording",
            "display_name": {"zh": "停止录制", "en": "Stop Recording"},
            "description": {"zh": "停止视频录制", "en": "Stop video recording"},
            "parameters": "{}",
            "is_required": false
        }
    ]', '[]', NULL, 1, 1, NULL, datetime('now'), datetime('now')),
('builtin-modbus-rtu-device', 'modbus_rtu_device', '{"zh": "Modbus RTU设备", "en": "Modbus RTU Device"}', '{"zh": "标准Modbus RTU协议设备模板，支持寄存器读写操作", "en": "Standard Modbus RTU protocol device template with register read/write operations"}', '1.0.0', 'System', 'controllers', NULL, 'device', 'modbus', 'modbus_rtu', '["modbus", "rtu", "controller", "industrial"]', '{"default_name_pattern": "modbus_device_{index}", "default_display_name_pattern": "Modbus设备 {index}", "default_description": {"zh": "Modbus RTU工业设备", "en": "Modbus RTU Industrial Device"}, "required_fields": ["name", "address"]}', '[
        {
            "name": "holding_register_1",
            "display_name": {"zh": "保持寄存器1", "en": "Holding Register 1"},
            "description": {"zh": "保持寄存器地址1的值", "en": "Value of holding register address 1"},
            "data_type": "number",
            "min_value": 0.0,
            "max_value": 65535.0,
            "default_value": "0",
            "is_read_only": false,
            "is_required": false
        },
        {
            "name": "input_register_1",
            "display_name": {"zh": "输入寄存器1", "en": "Input Register 1"},
            "description": {"zh": "输入寄存器地址1的值", "en": "Value of input register address 1"},
            "data_type": "number",
            "min_value": 0.0,
            "max_value": 65535.0,
            "default_value": "0",
            "is_read_only": true,
            "is_required": false
        },
        {
            "name": "coil_1",
            "display_name": {"zh": "线圈1", "en": "Coil 1"},
            "description": {"zh": "线圈地址1的状态", "en": "Status of coil address 1"},
            "data_type": "boolean",
            "default_value": "false",
            "is_read_only": false,
            "is_required": false
        },
        {
            "name": "discrete_input_1",
            "display_name": {"zh": "离散输入1", "en": "Discrete Input 1"},
            "description": {"zh": "离散输入地址1的状态", "en": "Status of discrete input address 1"},
            "data_type": "boolean",
            "default_value": "false",
            "is_read_only": true,
            "is_required": false
        }
    ]', '[
        {
            "name": "read_holding_registers",
            "display_name": {"zh": "读取保持寄存器", "en": "Read Holding Registers"},
            "description": {"zh": "读取指定地址的保持寄存器", "en": "Read holding registers at specified address"},
            "parameters": "{\"address\": 1, \"count\": 1}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"address\": {\"type\": \"number\", \"minimum\": 0, \"maximum\": 65535}, \"count\": {\"type\": \"number\", \"minimum\": 1, \"maximum\": 125}}, \"required\": [\"address\", \"count\"]}",
            "is_required": true
        },
        {
            "name": "write_single_register",
            "display_name": {"zh": "写入单个寄存器", "en": "Write Single Register"},
            "description": {"zh": "写入单个保持寄存器的值", "en": "Write value to single holding register"},
            "parameters": "{\"address\": 1, \"value\": 0}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"address\": {\"type\": \"number\", \"minimum\": 0, \"maximum\": 65535}, \"value\": {\"type\": \"number\", \"minimum\": 0, \"maximum\": 65535}}, \"required\": [\"address\", \"value\"]}",
            "is_required": false
        },
        {
            "name": "read_coils",
            "display_name": {"zh": "读取线圈", "en": "Read Coils"},
            "description": {"zh": "读取指定地址的线圈状态", "en": "Read coil status at specified address"},
            "parameters": "{\"address\": 1, \"count\": 1}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"address\": {\"type\": \"number\", \"minimum\": 0, \"maximum\": 65535}, \"count\": {\"type\": \"number\", \"minimum\": 1, \"maximum\": 2000}}, \"required\": [\"address\", \"count\"]}",
            "is_required": false
        },
        {
            "name": "write_single_coil",
            "display_name": {"zh": "写入单个线圈", "en": "Write Single Coil"},
            "description": {"zh": "写入单个线圈的状态", "en": "Write status to single coil"},
            "parameters": "{\"address\": 1, \"value\": false}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"address\": {\"type\": \"number\", \"minimum\": 0, \"maximum\": 65535}, \"value\": {\"type\": \"boolean\"}}, \"required\": [\"address\", \"value\"]}",
            "is_required": false
        }
    ]', '[]', NULL, 1, 1, NULL, datetime('now'), datetime('now')),
('builtin-mqtt-device', 'mqtt_device', '{"zh": "MQTT设备", "en": "MQTT Device"}', '{"zh": "标准MQTT协议设备模板，支持发布和订阅消息", "en": "Standard MQTT protocol device template with publish and subscribe capabilities"}', '1.0.0', 'System', 'gateways', NULL, 'device', 'mqtt', 'mqtt', '["mqtt", "iot", "gateway", "messaging"]', '{"default_name_pattern": "mqtt_device_{index}", "default_display_name_pattern": "MQTT设备 {index}", "default_description": {"zh": "MQTT物联网设备", "en": "MQTT IoT Device"}, "required_fields": ["name", "address"]}', '[
        {
            "name": "connection_status",
            "display_name": {"zh": "连接状态", "en": "Connection Status"},
            "description": {"zh": "MQTT连接状态", "en": "MQTT connection status"},
            "data_type": "string",
            "default_value": "disconnected",
            "is_read_only": true,
            "is_required": true
        },
        {
            "name": "last_message_time",
            "display_name": {"zh": "最后消息时间", "en": "Last Message Time"},
            "description": {"zh": "最后收到消息的时间", "en": "Time of last received message"},
            "data_type": "string",
            "is_read_only": true,
            "is_required": false
        },
        {
            "name": "message_count",
            "display_name": {"zh": "消息计数", "en": "Message Count"},
            "description": {"zh": "收到的消息总数", "en": "Total number of received messages"},
            "data_type": "number",
            "min_value": 0.0,
            "default_value": "0",
            "is_read_only": true,
            "is_required": false
        },
        {
            "name": "qos_level",
            "display_name": {"zh": "QoS级别", "en": "QoS Level"},
            "description": {"zh": "消息质量服务级别", "en": "Message Quality of Service level"},
            "data_type": "number",
            "min_value": 0.0,
            "max_value": 2.0,
            "default_value": "1",
            "is_read_only": false,
            "is_required": false
        }
    ]', '[
        {
            "name": "publish_message",
            "display_name": {"zh": "发布消息", "en": "Publish Message"},
            "description": {"zh": "向指定主题发布消息", "en": "Publish message to specified topic"},
            "parameters": "{\"topic\": \"device/data\", \"payload\": \"{}\", \"qos\": 1}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"topic\": {\"type\": \"string\", \"minLength\": 1}, \"payload\": {\"type\": \"string\"}, \"qos\": {\"type\": \"number\", \"minimum\": 0, \"maximum\": 2}}, \"required\": [\"topic\", \"payload\"]}",
            "is_required": true
        },
        {
            "name": "subscribe_topic",
            "display_name": {"zh": "订阅主题", "en": "Subscribe Topic"},
            "description": {"zh": "订阅指定的MQTT主题", "en": "Subscribe to specified MQTT topic"},
            "parameters": "{\"topic\": \"device/command\", \"qos\": 1}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"topic\": {\"type\": \"string\", \"minLength\": 1}, \"qos\": {\"type\": \"number\", \"minimum\": 0, \"maximum\": 2}}, \"required\": [\"topic\"]}",
            "is_required": false
        },
        {
            "name": "unsubscribe_topic",
            "display_name": {"zh": "取消订阅", "en": "Unsubscribe Topic"},
            "description": {"zh": "取消订阅指定的MQTT主题", "en": "Unsubscribe from specified MQTT topic"},
            "parameters": "{\"topic\": \"device/command\"}",
            "parameter_schema": "{\"type\": \"object\", \"properties\": {\"topic\": {\"type\": \"string\", \"minLength\": 1}}, \"required\": [\"topic\"]}",
            "is_required": false
        },
        {
            "name": "get_connection_info",
            "display_name": {"zh": "获取连接信息", "en": "Get Connection Info"},
            "description": {"zh": "获取MQTT连接详细信息", "en": "Get detailed MQTT connection information"},
            "parameters": "{}",
            "is_required": false
        }
    ]', '[]', NULL, 1, 1, NULL, datetime('now'), datetime('now')),
('builtin-smart-switch', 'smart_switch', '{"zh": "智能开关", "en": "Smart Switch"}', '{"zh": "智能开关设备模板，支持开关状态控制和电流监测", "en": "Smart switch device template with switch control and current monitoring"}', '1.0.0', 'System', 'controllers', NULL, 'device', 'wifi', 'wifi_switch', '["switch", "smart", "control", "power"]', '{"default_name_pattern": "switch_{index}", "default_display_name_pattern": "智能开关 {index}", "default_description": {"zh": "智能电源开关", "en": "Smart Power Switch"}, "required_fields": ["name", "address"]}', '[
        {
            "name": "switch_state",
            "display_name": {"zh": "开关状态", "en": "Switch State"},
            "description": {"zh": "开关的当前状态", "en": "Current state of the switch"},
            "data_type": "boolean",
            "default_value": "false",
            "is_read_only": false,
            "is_required": true
        },
        {
            "name": "current",
            "display_name": {"zh": "电流", "en": "Current"},
            "description": {"zh": "当前电流值", "en": "Current electrical current"},
            "data_type": "number",
            "unit": "A",
            "min_value": 0.0,
            "max_value": 16.0,
            "default_value": "0.0",
            "is_read_only": true,
            "is_required": false
        },
        {
            "name": "power",
            "display_name": {"zh": "功率", "en": "Power"},
            "description": {"zh": "当前功率消耗", "en": "Current power consumption"},
            "data_type": "number",
            "unit": "W",
            "min_value": 0.0,
            "max_value": 3680.0,
            "default_value": "0.0",
            "is_read_only": true,
            "is_required": false
        },
        {
            "name": "energy_today",
            "display_name": {"zh": "今日用电量", "en": "Energy Today"},
            "description": {"zh": "今日累计用电量", "en": "Total energy consumption today"},
            "data_type": "number",
            "unit": "kWh",
            "min_value": 0.0,
            "default_value": "0.0",
            "is_read_only": true,
            "is_required": false
        }
    ]', '[
        {
            "name": "turn_on",
            "display_name": {"zh": "打开开关", "en": "Turn On"},
            "description": {"zh": "打开智能开关", "en": "Turn on the smart switch"},
            "parameters": "{}",
            "is_required": true
        },
        {
            "name": "turn_off",
            "display_name": {"zh": "关闭开关", "en": "Turn Off"},
            "description": {"zh": "关闭智能开关", "en": "Turn off the smart switch"},
            "parameters": "{}",
            "is_required": true
        },
        {
            "name": "toggle",
            "display_name": {"zh": "切换状态", "en": "Toggle"},
            "description": {"zh": "切换开关状态", "en": "Toggle switch state"},
            "parameters": "{}",
            "is_required": false
        },
        {
            "name": "get_power_stats",
            "display_name": {"zh": "获取电力统计", "en": "Get Power Statistics"},
            "description": {"zh": "获取详细的电力使用统计", "en": "Get detailed power usage statistics"},
            "parameters": "{}",
            "is_required": false
        },
        {
            "name": "reset_energy_counter",
            "display_name": {"zh": "重置电量计数", "en": "Reset Energy Counter"},
            "description": {"zh": "重置累计电量计数器", "en": "Reset cumulative energy counter"},
            "parameters": "{}",
            "is_required": false
        }
    ]', '[]', NULL, 1, 1, NULL, datetime('now'), datetime('now')),
('builtin_temperature_humidity_sensor', 'temperature_humidity_sensor', '{"zh":"温湿度传感器","en":"Temperature & Humidity Sensor"}', '{"zh":"高精度温湿度传感器，适用于机房、仓库、农业大棚等场景","en":"High-precision temperature and humidity sensor, suitable for server rooms, warehouses, greenhouses"}', '1.0.0', 'TinyIoT', 'sensors', 'TinyIoT', 'device', 'modbus', 'modbus_rtu', '["temperature","humidity","environment"]', '{"default_name_pattern":"th_sensor_{index}","default_display_name_pattern":{"zh":"温湿度传感器 {index}","en":"Temp & Humidity Sensor {index}"},"required_fields":["name","address"]}', '[{"name":"temperature","display_name":{"zh":"温度","en":"Temperature"},"description":{"zh":"环境温度","en":"Ambient temperature"},"data_type":"number","unit":"°C","min_value":-40.0,"max_value":80.0,"default_value":"25.0","is_read_only":true,"is_required":true},{"name":"humidity","display_name":{"zh":"湿度","en":"Humidity"},"description":{"zh":"相对湿度","en":"Relative humidity"},"data_type":"number","unit":"%RH","min_value":0.0,"max_value":100.0,"default_value":"50.0","is_read_only":true,"is_required":true},{"name":"dew_point","display_name":{"zh":"露点温度","en":"Dew Point"},"description":{"zh":"露点温度","en":"Dew point temperature"},"data_type":"number","unit":"°C","min_value":-40.0,"max_value":80.0,"default_value":"15.0","is_read_only":true,"is_required":false}]', '[{"name":"read_all","display_name":{"zh":"读取全部","en":"Read All"},"description":{"zh":"读取温湿度数据","en":"Read temperature and humidity data"},"parameters":"{}","is_required":true}]', '[]', NULL, 1, 1, NULL, '2024-01-01 00:00:00', '2024-01-01 00:00:00'),
('builtin_power_meter', 'power_meter', '{"zh":"电力仪表","en":"Power Meter"}', '{"zh":"多功能电力仪表，监测电压、电流、功率、电量，适用于配电柜、能耗监测","en":"Multi-function power meter, monitors voltage, current, power, energy, for distribution panels and energy monitoring"}', '1.0.0', 'TinyIoT', 'sensors', 'TinyIoT', 'device', 'modbus', 'modbus_rtu', '["power","energy","electricity","meter"]', '{"default_name_pattern":"power_meter_{index}","default_display_name_pattern":{"zh":"电力仪表 {index}","en":"Power Meter {index}"},"required_fields":["name","address"]}', '[{"name":"voltage","display_name":{"zh":"电压","en":"Voltage"},"description":{"zh":"相电压","en":"Phase voltage"},"data_type":"number","unit":"V","min_value":0.0,"max_value":500.0,"default_value":"220.0","is_read_only":true,"is_required":true},{"name":"current","display_name":{"zh":"电流","en":"Current"},"description":{"zh":"相电流","en":"Phase current"},"data_type":"number","unit":"A","min_value":0.0,"max_value":1000.0,"default_value":"10.0","is_read_only":true,"is_required":true},{"name":"active_power","display_name":{"zh":"有功功率","en":"Active Power"},"description":{"zh":"有功功率","en":"Active power"},"data_type":"number","unit":"kW","min_value":0.0,"max_value":999999.0,"default_value":"2.2","is_read_only":true,"is_required":true},{"name":"energy","display_name":{"zh":"累计电量","en":"Total Energy"},"description":{"zh":"累计用电量","en":"Total energy consumption"},"data_type":"number","unit":"kWh","min_value":0.0,"max_value":99999999.0,"default_value":"0.0","is_read_only":true,"is_required":true},{"name":"power_factor","display_name":{"zh":"功率因数","en":"Power Factor"},"description":{"zh":"功率因数","en":"Power factor"},"data_type":"number","unit":"","min_value":-1.0,"max_value":1.0,"default_value":"0.95","is_read_only":true,"is_required":false},{"name":"frequency","display_name":{"zh":"频率","en":"Frequency"},"description":{"zh":"电网频率","en":"Grid frequency"},"data_type":"number","unit":"Hz","min_value":45.0,"max_value":65.0,"default_value":"50.0","is_read_only":true,"is_required":false}]', '[{"name":"read_all","display_name":{"zh":"读取全部","en":"Read All"},"description":{"zh":"读取所有电力参数","en":"Read all power parameters"},"parameters":"{}","is_required":true},{"name":"reset_energy","display_name":{"zh":"电量清零","en":"Reset Energy"},"description":{"zh":"重置累计电量","en":"Reset total energy counter"},"parameters":"{}","is_required":false}]', '[]', NULL, 1, 1, NULL, '2024-01-01 00:00:00', '2024-01-01 00:00:00'),
('builtin_water_level_sensor', 'water_level_sensor', '{"zh":"液位传感器","en":"Water Level Sensor"}', '{"zh":"投入式液位传感器，适用于水箱、水池、河道水位监测","en":"Submersible level sensor, suitable for tanks, pools, river level monitoring"}', '1.0.0', 'TinyIoT', 'sensors', 'TinyIoT', 'device', 'modbus', 'modbus_rtu', '["water","level","liquid","tank"]', '{"default_name_pattern":"water_level_{index}","default_display_name_pattern":{"zh":"液位传感器 {index}","en":"Water Level Sensor {index}"},"required_fields":["name","address"]}', '[{"name":"level","display_name":{"zh":"液位","en":"Level"},"description":{"zh":"当前液位高度","en":"Current liquid level"},"data_type":"number","unit":"m","min_value":0.0,"max_value":10.0,"default_value":"1.5","is_read_only":true,"is_required":true},{"name":"level_percent","display_name":{"zh":"液位百分比","en":"Level Percentage"},"description":{"zh":"液位百分比","en":"Level percentage"},"data_type":"number","unit":"%","min_value":0.0,"max_value":100.0,"default_value":"50.0","is_read_only":true,"is_required":true},{"name":"temperature","display_name":{"zh":"液体温度","en":"Liquid Temperature"},"description":{"zh":"液体温度","en":"Liquid temperature"},"data_type":"number","unit":"°C","min_value":-10.0,"max_value":80.0,"default_value":"20.0","is_read_only":true,"is_required":false}]', '[{"name":"read_all","display_name":{"zh":"读取全部","en":"Read All"},"description":{"zh":"读取液位数据","en":"Read level data"},"parameters":"{}","is_required":true}]', '[]', NULL, 1, 1, NULL, '2024-01-01 00:00:00', '2024-01-01 00:00:00'),
('builtin_air_quality_sensor', 'air_quality_sensor', '{"zh":"空气质量传感器","en":"Air Quality Sensor"}', '{"zh":"多合一空气质量传感器，监测PM2.5、CO2、温度、湿度，适用于智慧楼宇","en":"Multi-in-one air quality sensor, monitors PM2.5, CO2, temperature, humidity, for smart buildings"}', '1.0.0', 'TinyIoT', 'sensors', 'TinyIoT', 'device', 'modbus', 'modbus_rtu', '["air","quality","pm25","co2","indoor"]', '{"default_name_pattern":"air_quality_{index}","default_display_name_pattern":{"zh":"空气质量传感器 {index}","en":"Air Quality Sensor {index}"},"required_fields":["name","address"]}', '[{"name":"pm25","display_name":{"zh":"PM2.5","en":"PM2.5"},"description":{"zh":"细颗粒物浓度","en":"PM2.5 concentration"},"data_type":"number","unit":"μg/m³","min_value":0.0,"max_value":1000.0,"default_value":"35.0","is_read_only":true,"is_required":true},{"name":"pm10","display_name":{"zh":"PM10","en":"PM10"},"description":{"zh":"可吸入颗粒物浓度","en":"PM10 concentration"},"data_type":"number","unit":"μg/m³","min_value":0.0,"max_value":1000.0,"default_value":"50.0","is_read_only":true,"is_required":false},{"name":"co2","display_name":{"zh":"CO2浓度","en":"CO2 Level"},"description":{"zh":"二氧化碳浓度","en":"Carbon dioxide concentration"},"data_type":"number","unit":"ppm","min_value":400.0,"max_value":5000.0,"default_value":"400.0","is_read_only":true,"is_required":true},{"name":"temperature","display_name":{"zh":"温度","en":"Temperature"},"description":{"zh":"环境温度","en":"Ambient temperature"},"data_type":"number","unit":"°C","min_value":-20.0,"max_value":60.0,"default_value":"25.0","is_read_only":true,"is_required":true},{"name":"humidity","display_name":{"zh":"湿度","en":"Humidity"},"description":{"zh":"相对湿度","en":"Relative humidity"},"data_type":"number","unit":"%RH","min_value":0.0,"max_value":100.0,"default_value":"50.0","is_read_only":true,"is_required":true},{"name":"tvoc","display_name":{"zh":"TVOC","en":"TVOC"},"description":{"zh":"总挥发性有机化合物","en":"Total volatile organic compounds"},"data_type":"number","unit":"ppb","min_value":0.0,"max_value":50000.0,"default_value":"220.0","is_read_only":true,"is_required":false},{"name":"aqi","display_name":{"zh":"AQI","en":"AQI"},"description":{"zh":"空气质量指数","en":"Air quality index"},"data_type":"number","unit":"","min_value":0.0,"max_value":500.0,"default_value":"50.0","is_read_only":true,"is_required":true}]', '[{"name":"read_all","display_name":{"zh":"读取全部","en":"Read All"},"description":{"zh":"读取所有空气质量数据","en":"Read all air quality data"},"parameters":"{}","is_required":true},{"name":"calibrate","display_name":{"zh":"校准","en":"Calibrate"},"description":{"zh":"执行传感器校准","en":"Execute sensor calibration"},"parameters":"{\"sensor\": \"co2\"}","parameter_schema":"{\"type\": \"object\", \"properties\": {\"sensor\": {\"type\": \"string\", \"enum\": [\"pm25\", \"co2\", \"tvoc\"]}}, \"required\": [\"sensor\"]}","is_required":false}]', '[]', NULL, 1, 1, NULL, '2024-01-01 00:00:00', '2024-01-01 00:00:00'),
('builtin_ip_camera', 'ip_camera', '{"zh":"网络摄像头","en":"IP Camera"}', '{"zh":"ONVIF协议网络摄像头，支持RTSP流、PTZ控制、移动侦测","en":"ONVIF IP camera, supports RTSP streaming, PTZ control, motion detection"}', '1.0.0', 'TinyIoT', 'cameras', 'Generic', 'device', 'onvif', 'onvif', '["camera","video","surveillance","onvif"]', '{"default_name_pattern":"camera_{index}","default_display_name_pattern":{"zh":"摄像头 {index}","en":"IP Camera {index}"},"required_fields":["name","address"]}', '[{"name":"rtsp_url","display_name":{"zh":"RTSP地址","en":"RTSP URL"},"description":{"zh":"视频流地址","en":"Video stream URL"},"data_type":"string","unit":"","default_value":"","is_read_only":true,"is_required":true},{"name":"resolution","display_name":{"zh":"分辨率","en":"Resolution"},"description":{"zh":"视频分辨率","en":"Video resolution"},"data_type":"string","unit":"","default_value":"1920x1080","is_read_only":true,"is_required":false},{"name":"motion_detected","display_name":{"zh":"移动侦测","en":"Motion Detected"},"description":{"zh":"是否检测到移动","en":"Whether motion is detected"},"data_type":"boolean","unit":"","default_value":"false","is_read_only":true,"is_required":false},{"name":"ptz_pan","display_name":{"zh":"水平角度","en":"Pan Angle"},"description":{"zh":"PTZ水平角度","en":"PTZ pan angle"},"data_type":"number","unit":"°","min_value":-180.0,"max_value":180.0,"default_value":"0","is_read_only":false,"is_required":false},{"name":"ptz_tilt","display_name":{"zh":"俯仰角度","en":"Tilt Angle"},"description":{"zh":"PTZ俯仰角度","en":"PTZ tilt angle"},"data_type":"number","unit":"°","min_value":-90.0,"max_value":90.0,"default_value":"0","is_read_only":false,"is_required":false}]', '[{"name":"snapshot","display_name":{"zh":"截图","en":"Snapshot"},"description":{"zh":"抓取当前画面","en":"Capture current frame"},"parameters":"{}","is_required":true},{"name":"ptz_move","display_name":{"zh":"PTZ移动","en":"PTZ Move"},"description":{"zh":"控制云台移动","en":"Control PTZ movement"},"parameters":"{\"direction\": \"left\"}","parameter_schema":"{\"type\": \"object\", \"properties\": {\"direction\": {\"type\": \"string\", \"enum\": [\"left\", \"right\", \"up\", \"down\", \"stop\"]}}, \"required\": [\"direction\"]}","is_required":false},{"name":"ptz_home","display_name":{"zh":"回到原点","en":"PTZ Home"},"description":{"zh":"云台回到初始位置","en":"Move PTZ to home position"},"parameters":"{}","is_required":false}]', '[]', NULL, 1, 1, NULL, '2024-01-01 00:00:00', '2024-01-01 00:00:00'),
('builtin_agv_robot', 'agv_robot', '{"zh":"AGV搬运机器人","en":"AGV Robot"}', '{"zh":"自动导引搬运车，支持任务调度、状态监控、电量管理","en":"Automated Guided Vehicle, supports task dispatch, status monitoring, battery management"}', '1.0.0', 'TinyIoT', 'robots', 'Generic', 'device', 'mqtt', 'mqtt', '["agv","robot","logistics","transport"]', '{"default_name_pattern":"agv_{index}","default_display_name_pattern":{"zh":"AGV机器人 {index}","en":"AGV Robot {index}"},"required_fields":["name","address"]}', '[{"name":"status","display_name":{"zh":"运行状态","en":"Status"},"description":{"zh":"当前运行状态","en":"Current running status"},"data_type":"string","unit":"","default_value":"idle","is_read_only":true,"is_required":true},{"name":"battery_level","display_name":{"zh":"电量","en":"Battery Level"},"description":{"zh":"电池电量百分比","en":"Battery level percentage"},"data_type":"number","unit":"%","min_value":0.0,"max_value":100.0,"default_value":"80.0","is_read_only":true,"is_required":true},{"name":"position_x","display_name":{"zh":"X坐标","en":"X Position"},"description":{"zh":"当前X坐标","en":"Current X position"},"data_type":"number","unit":"m","min_value":-100.0,"max_value":100.0,"default_value":"0.0","is_read_only":true,"is_required":true},{"name":"position_y","display_name":{"zh":"Y坐标","en":"Y Position"},"description":{"zh":"当前Y坐标","en":"Current Y position"},"data_type":"number","unit":"m","min_value":-100.0,"max_value":100.0,"default_value":"0.0","is_read_only":true,"is_required":true},{"name":"speed","display_name":{"zh":"速度","en":"Speed"},"description":{"zh":"当前移动速度","en":"Current movement speed"},"data_type":"number","unit":"m/s","min_value":0.0,"max_value":5.0,"default_value":"0.0","is_read_only":true,"is_required":false},{"name":"current_task","display_name":{"zh":"当前任务","en":"Current Task"},"description":{"zh":"当前执行的任务ID","en":"Current task ID"},"data_type":"string","unit":"","default_value":"","is_read_only":true,"is_required":false}]', '[{"name":"move_to","display_name":{"zh":"移动到","en":"Move To"},"description":{"zh":"移动到指定坐标","en":"Move to specified position"},"parameters":"{\"x\": 10.0, \"y\": 5.0}","parameter_schema":"{\"type\": \"object\", \"properties\": {\"x\": {\"type\": \"number\"}, \"y\": {\"type\": \"number\"}}, \"required\": [\"x\", \"y\"]}","is_required":true},{"name":"stop","display_name":{"zh":"停止","en":"Stop"},"description":{"zh":"立即停止移动","en":"Stop movement immediately"},"parameters":"{}","is_required":true},{"name":"charge","display_name":{"zh":"回充电桩","en":"Return to Charge"},"description":{"zh":"返回充电桩充电","en":"Return to charging station"},"parameters":"{}","is_required":false}]', '[]', NULL, 1, 1, NULL, '2024-01-01 00:00:00', '2024-01-01 00:00:00'),
('builtin_plc_controller', 'plc_controller', '{"zh":"PLC控制器","en":"PLC Controller"}', '{"zh":"Modbus协议PLC控制器，适用于工业自动化、产线控制","en":"Modbus PLC controller, suitable for industrial automation and production line control"}', '1.0.0', 'TinyIoT', 'controllers', 'Generic', 'device', 'modbus', 'modbus_rtu', '["plc","controller","industrial","automation"]', '{"default_name_pattern":"plc_{index}","default_display_name_pattern":{"zh":"PLC控制器 {index}","en":"PLC Controller {index}"},"required_fields":["name","address"]}', '[{"name":"run_status","display_name":{"zh":"运行状态","en":"Run Status"},"description":{"zh":"PLC运行状态","en":"PLC running status"},"data_type":"string","unit":"","default_value":"running","is_read_only":true,"is_required":true},{"name":"error_code","display_name":{"zh":"错误码","en":"Error Code"},"description":{"zh":"当前错误码","en":"Current error code"},"data_type":"integer","unit":"","min_value":0,"max_value":9999,"default_value":"0","is_read_only":true,"is_required":true},{"name":"cycle_time","display_name":{"zh":"扫描周期","en":"Cycle Time"},"description":{"zh":"程序扫描周期","en":"Program scan cycle time"},"data_type":"number","unit":"ms","min_value":0.0,"max_value":10000.0,"default_value":"10.0","is_read_only":true,"is_required":false},{"name":"input_1","display_name":{"zh":"输入点1","en":"Input 1"},"description":{"zh":"数字输入点1","en":"Digital input 1"},"data_type":"boolean","unit":"","default_value":"false","is_read_only":true,"is_required":false},{"name":"output_1","display_name":{"zh":"输出点1","en":"Output 1"},"description":{"zh":"数字输出点1","en":"Digital output 1"},"data_type":"boolean","unit":"","default_value":"false","is_read_only":false,"is_required":false}]', '[{"name":"read_status","display_name":{"zh":"读取状态","en":"Read Status"},"description":{"zh":"读取PLC运行状态","en":"Read PLC running status"},"parameters":"{}","is_required":true},{"name":"start","display_name":{"zh":"启动","en":"Start"},"description":{"zh":"启动PLC程序","en":"Start PLC program"},"parameters":"{}","is_required":false},{"name":"stop","display_name":{"zh":"停止","en":"Stop"},"description":{"zh":"停止PLC程序","en":"Stop PLC program"},"parameters":"{}","is_required":false},{"name":"reset","display_name":{"zh":"复位","en":"Reset"},"description":{"zh":"复位PLC错误","en":"Reset PLC error"},"parameters":"{}","is_required":false}]', '[]', NULL, 1, 1, NULL, '2024-01-01 00:00:00', '2024-01-01 00:00:00'),
('builtin_smart_relay', 'smart_relay', '{"zh":"智能继电器","en":"Smart Relay"}', '{"zh":"多路智能继电器模块，支持远程开关控制，适用于照明、门禁、设备启停","en":"Multi-channel smart relay module, supports remote on/off control, for lighting, access control, device switching"}', '1.0.0', 'TinyIoT', 'controllers', 'TinyIoT', 'device', 'modbus', 'modbus_rtu', '["relay","switch","control","lighting"]', '{"default_name_pattern":"relay_{index}","default_display_name_pattern":{"zh":"智能继电器 {index}","en":"Smart Relay {index}"},"required_fields":["name","address"]}', '[{"name":"channel_1","display_name":{"zh":"通道1","en":"Channel 1"},"description":{"zh":"继电器通道1状态","en":"Relay channel 1 status"},"data_type":"boolean","unit":"","default_value":"false","is_read_only":false,"is_required":true},{"name":"channel_2","display_name":{"zh":"通道2","en":"Channel 2"},"description":{"zh":"继电器通道2状态","en":"Relay channel 2 status"},"data_type":"boolean","unit":"","default_value":"false","is_read_only":false,"is_required":true},{"name":"channel_3","display_name":{"zh":"通道3","en":"Channel 3"},"description":{"zh":"继电器通道3状态","en":"Relay channel 3 status"},"data_type":"boolean","unit":"","default_value":"false","is_read_only":false,"is_required":false},{"name":"channel_4","display_name":{"zh":"通道4","en":"Channel 4"},"description":{"zh":"继电器通道4状态","en":"Relay channel 4 status"},"data_type":"boolean","unit":"","default_value":"false","is_read_only":false,"is_required":false}]', '[{"name":"set_channel","display_name":{"zh":"设置通道","en":"Set Channel"},"description":{"zh":"设置指定通道开关状态","en":"Set specified channel on/off"},"parameters":"{\"channel\": 1, \"value\": true}","parameter_schema":"{\"type\": \"object\", \"properties\": {\"channel\": {\"type\": \"integer\", \"minimum\": 1, \"maximum\": 4}, \"value\": {\"type\": \"boolean\"}}, \"required\": [\"channel\", \"value\"]}","is_required":true},{"name":"all_off","display_name":{"zh":"全部关闭","en":"All Off"},"description":{"zh":"关闭所有通道","en":"Turn off all channels"},"parameters":"{}","is_required":false}]', '[]', NULL, 1, 1, NULL, '2024-01-01 00:00:00', '2024-01-01 00:00:00');

-- ── 场景包模板（source: templates/builtin/scenes/*.json）────────────────────
-- device_info 列存完整模板 JSON 原文；根级非空 children 即组合模板。
INSERT OR IGNORE INTO thing_templates (
    id, name, display_name, description, version, author, category, manufacturer,
    thing_type, protocol_type, driver_name, tags, device_info, properties, actions,
    events, default_knowledge, is_builtin, is_active, workspace_id, created_at, updated_at
)
SELECT
  'builtin_smart_floor', 'smart_floor', '{"zh": "智慧楼层", "en": "Smart Floor"}',
  '{"zh": "一层楼的空间结构：楼层 + N 个房间", "en": "Floor structure with N rooms"}', '1.0.0', 'TinyIoT', 'scenes', 'TinyIoT', 'space', NULL, NULL,
  '["floor", "space"]',
  '{
  "name": "smart_floor",
  "display_name": {"zh": "智慧楼层", "en": "Smart Floor"},
  "description": {"zh": "一层楼的空间结构：楼层 + N 个房间", "en": "Floor structure with N rooms"},
  "version": "1.0.0",
  "author": "TinyIoT",
  "category": "scenes",
  "thing_category": "floor",
  "tags": ["floor", "space"],
  "parameters": [
    {"name": "room_count", "type": "int", "default": 8, "min": 1, "max": 50,
     "display_name": {"zh": "房间数量", "en": "Room Count"}}
  ],
  "device_info": {
    "default_name_pattern": "{scene_name}",
    "default_display_name_pattern": {"zh": "{scene_name}", "en": "{scene_name}"}
  },
  "default_knowledge": "你是这层楼的楼层管家，关注各房间环境与设备状态。",
  "properties": [
    {"name": "area", "display_name": {"zh": "面积", "en": "Area"}, "data_type": "number", "unit": "m²", "is_read_only": false, "is_required": false}
  ],
  "children": [
    {"key": "room", "category": "room", "count_param": "room_count",
     "device_info": {"default_name_pattern": "{index}室",
       "default_display_name_pattern": {"zh": "{index}室", "en": "Room {index}"}}}
  ]
}',
  '[]', '[]', '[]', '你是这层楼的楼层管家，关注各房间环境与设备状态。', 1, 1, NULL, datetime('now'), datetime('now')
WHERE NOT EXISTS (SELECT 1 FROM thing_templates WHERE id = 'builtin_smart_floor');

INSERT OR IGNORE INTO thing_templates (
    id, name, display_name, description, version, author, category, manufacturer,
    thing_type, protocol_type, driver_name, tags, device_info, properties, actions,
    events, default_knowledge, is_builtin, is_active, workspace_id, created_at, updated_at
)
SELECT
  'builtin_smart_building', 'smart_building', '{"zh": "智慧楼宇", "en": "Smart Building"}',
  '{"zh": "单体建筑：楼栋 + N 层 + 每层 2 个温湿度传感器", "en": "Building with N floors, 2 temp/humidity sensors per floor"}', '1.0.0', 'TinyIoT', 'scenes', 'TinyIoT', 'building', NULL, NULL,
  '["building", "space"]',
  '{
  "name": "smart_building",
  "display_name": {"zh": "智慧楼宇", "en": "Smart Building"},
  "description": {"zh": "单体建筑：楼栋 + N 层 + 每层 2 个温湿度传感器", "en": "Building with N floors, 2 temp/humidity sensors per floor"},
  "version": "1.0.0",
  "author": "TinyIoT",
  "category": "scenes",
  "thing_category": "building",
  "tags": ["building", "space"],
  "parameters": [
    {"name": "floor_count", "type": "int", "default": 10, "min": 1, "max": 15,
     "display_name": {"zh": "楼层数", "en": "Floor Count"}}
  ],
  "device_info": {"default_name_pattern": "{scene_name}"},
  "default_knowledge": "你是这栋楼的楼宇管家，关注各层环境与设备状态。",
  "children": [
    {"key": "floor", "category": "floor", "count_param": "floor_count",
     "device_info": {"default_name_pattern": "{index}F",
       "default_display_name_pattern": {"zh": "{index}F", "en": "{index}F"}},
     "resources": [
       {"name": "floor_plan", "type": "image", "uri": "builtin://scenes/smart_building/floor_plan.png"}
     ],
     "children": [
       {"key": "th_sensor", "template_ref": "temperature_humidity_sensor", "count": 2,
        "device_info": {"default_name_pattern": "th_sensor_{index}",
          "default_display_name_pattern": {"zh": "温湿度传感器 {index}", "en": "Temp & Humidity Sensor {index}"}},
        "alarm_rules": [
          {"name": "高温告警", "rule_type": "threshold",
           "condition": {"type": "threshold", "operator": "greater_than", "value": 35.0},
           "alarm_level": "warning", "notification_config": {}, "property_ref": "temperature"}
        ]}
     ]}
  ]
}',
  '[]', '[]', '[]', '你是这栋楼的楼宇管家，关注各层环境与设备状态。', 1, 1, NULL, datetime('now'), datetime('now')
WHERE NOT EXISTS (SELECT 1 FROM thing_templates WHERE id = 'builtin_smart_building');

INSERT OR IGNORE INTO thing_templates (
    id, name, display_name, description, version, author, category, manufacturer,
    thing_type, protocol_type, driver_name, tags, device_info, properties, actions,
    events, default_knowledge, is_builtin, is_active, workspace_id, created_at, updated_at
)
SELECT
  'builtin_smart_campus', 'smart_campus', '{"zh": "智慧园区", "en": "Smart Campus"}',
  '{"zh": "园区：N 栋楼、每栋 M 层、每层 2 个温湿度传感器", "en": "Campus with N buildings, M floors each, 2 sensors per floor"}', '1.0.0', 'TinyIoT', 'scenes', 'TinyIoT', 'space', NULL, NULL,
  '["campus", "building"]',
  '{
  "name": "smart_campus",
  "display_name": {"zh": "智慧园区", "en": "Smart Campus"},
  "description": {"zh": "园区：N 栋楼、每栋 M 层、每层 2 个温湿度传感器", "en": "Campus with N buildings, M floors each, 2 sensors per floor"},
  "version": "1.0.0",
  "author": "TinyIoT",
  "category": "scenes",
  "thing_category": "campus",
  "tags": ["campus", "building"],
  "parameters": [
    {"name": "building_count", "type": "int", "default": 2, "min": 1, "max": 10,
     "display_name": {"zh": "楼栋数量", "en": "Building Count"}},
    {"name": "floor_count", "type": "int", "default": 5, "min": 1, "max": 15,
     "display_name": {"zh": "每栋楼层数（最终节点数受 500 上限约束）", "en": "Floors per Building"}}
  ],
  "device_info": {"default_name_pattern": "{scene_name}"},
  "properties": [
    {"name": "area", "display_name": {"zh": "占地面积", "en": "Site Area"}, "data_type": "number", "unit": "m²", "is_read_only": false, "is_required": false},
    {"name": "plot_ratio", "display_name": {"zh": "容积率", "en": "Plot Ratio"}, "data_type": "number", "is_read_only": false, "is_required": false}
  ],
  "default_knowledge": "你是园区管家，统览各楼栋运行状态与告警。",
  "dashboard": {"cards": [{"property": "area"}, {"property": "plot_ratio"}]},
  "alarm_rules": [
    {"name": "能耗异常", "rule_type": "change",
     "condition": {"type": "change", "change_type": "any", "threshold": 50.0, "time_window": 300},
     "alarm_level": "warning", "notification_config": {}}
  ],
  "children": [
    {"key": "building", "category": "building", "count_param": "building_count",
     "device_info": {"default_name_pattern": "{index}号楼",
       "default_display_name_pattern": {"zh": "{index}号楼", "en": "Building {index}"}},
     "default_knowledge": "你是楼栋管家，关注本楼各层环境。",
     "alarm_rules": [
       {"name": "楼栋温度超阈值", "rule_type": "threshold",
        "condition": {"type": "threshold", "operator": "greater_than", "value": 35.0},
        "alarm_level": "warning", "notification_config": {}}
     ],
     "children": [
       {"key": "floor", "category": "floor", "count_param": "floor_count",
        "device_info": {"default_name_pattern": "{index}F"},
        "resources": [
          {"name": "floor_plan", "type": "image", "uri": "builtin://scenes/smart_campus/floor_plan.png"}
        ],
        "children": [
          {"key": "th_sensor", "template_ref": "temperature_humidity_sensor", "count": 2,
           "device_info": {"default_name_pattern": "th_sensor_{index}"},
           "alarm_rules": [
             {"name": "高温告警", "rule_type": "threshold",
              "condition": {"type": "threshold", "operator": "greater_than", "value": 35.0},
              "alarm_level": "warning", "notification_config": {}, "property_ref": "temperature"}
           ]}
        ]}
     ]}
  ]
}',
  '[]', '[]', '[]', '你是园区管家，统览各楼栋运行状态与告警。', 1, 1, NULL, datetime('now'), datetime('now')
WHERE NOT EXISTS (SELECT 1 FROM thing_templates WHERE id = 'builtin_smart_campus');
