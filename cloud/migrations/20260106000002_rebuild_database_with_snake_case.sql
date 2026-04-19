-- 重构数据库：使用 snake_case 命名规范
-- 这个迁移将重建整个数据库结构，遵循 Rust 和 SQL 最佳实践

-- 启用外键约束
PRAGMA foreign_keys = ON;

-- 删除所有旧表（按依赖关系顺序）
DROP TABLE IF EXISTS tag_bindings;
DROP TABLE IF EXISTS tags;
DROP TABLE IF EXISTS TagRelations;
DROP TABLE IF EXISTS Tags;
DROP TABLE IF EXISTS DeviceAlarms;
DROP TABLE IF EXISTS DeviceAlarmRules;
DROP TABLE IF EXISTS UserPermissions;
DROP TABLE IF EXISTS RolePermissions;
DROP TABLE IF EXISTS Permissions;
DROP TABLE IF EXISTS UserRoles;
DROP TABLE IF EXISTS DeviceCommands;
DROP TABLE IF EXISTS DeviceProperties;
DROP TABLE IF EXISTS Devices;
DROP TABLE IF EXISTS Components;
DROP TABLE IF EXISTS Products;
DROP TABLE IF EXISTS Organizations;
DROP TABLE IF EXISTS Messages;
DROP TABLE IF EXISTS DeviceEventTriggers;
DROP TABLE IF EXISTS Menus;
DROP TABLE IF EXISTS Roles;
DROP TABLE IF EXISTS Users;

-- ============================================================================
-- 用户和权限管理
-- ============================================================================

-- 用户表
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    email TEXT UNIQUE,
    phone TEXT,
    display_name TEXT,
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    parent_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_login_at TEXT,
    FOREIGN KEY (parent_id) REFERENCES users(id) ON DELETE SET NULL
);

-- 角色表
CREATE TABLE roles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    is_administrator BOOLEAN NOT NULL DEFAULT false,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 用户角色关联表
CREATE TABLE user_roles (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    role_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE,
    UNIQUE(user_id, role_id)
);

-- 权限表
CREATE TABLE permissions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    resource_type TEXT NOT NULL, -- 'device', 'user', 'system', etc.
    action TEXT NOT NULL, -- 'read', 'write', 'delete', 'admin'
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 角色权限关联表
CREATE TABLE role_permissions (
    id TEXT PRIMARY KEY,
    role_id TEXT NOT NULL,
    permission_id TEXT NOT NULL,
    target_id TEXT, -- 可选的目标资源ID
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE,
    FOREIGN KEY (permission_id) REFERENCES permissions(id) ON DELETE CASCADE,
    UNIQUE(role_id, permission_id, target_id)
);

-- 用户权限关联表（直接授权给用户的权限）
CREATE TABLE user_permissions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    permission_id TEXT NOT NULL,
    target_id TEXT, -- 可选的目标资源ID
    expires_at TEXT, -- 可选的过期时间
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (permission_id) REFERENCES permissions(id) ON DELETE CASCADE,
    UNIQUE(user_id, permission_id, target_id)
);

-- ============================================================================
-- 组织和产品管理
-- ============================================================================

-- 组织表
CREATE TABLE organizations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    parent_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (parent_id) REFERENCES organizations(id) ON DELETE SET NULL
);

-- 产品表
CREATE TABLE products (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    version TEXT,
    manufacturer TEXT,
    device_type TEXT,
    protocol_type TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ============================================================================
-- 设备管理
-- ============================================================================

-- 设备表
CREATE TABLE devices (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    display_name TEXT,
    device_type TEXT,
    address TEXT,
    description TEXT,
    position TEXT,
    driver_name TEXT,
    device_model TEXT,
    protocol_type TEXT,
    factory_name TEXT,
    linked_data TEXT, -- JSON 字符串
    driver_options TEXT, -- JSON 字符串
    state INTEGER NOT NULL DEFAULT 0, -- 0: offline, 1: online, 2: alarm, 3: error
    parent_id TEXT,
    product_id TEXT,
    organization_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (parent_id) REFERENCES devices(id) ON DELETE SET NULL,
    FOREIGN KEY (product_id) REFERENCES products(id) ON DELETE SET NULL,
    FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE SET NULL
);

-- 设备属性表
CREATE TABLE device_properties (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    name TEXT NOT NULL,
    display_name TEXT,
    description TEXT,
    data_type TEXT NOT NULL, -- 'number', 'string', 'boolean', 'object'
    unit TEXT,
    min_value REAL,
    max_value REAL,
    default_value TEXT,
    is_read_only BOOLEAN NOT NULL DEFAULT false,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE,
    UNIQUE(device_id, name)
);

-- 设备命令表
CREATE TABLE device_commands (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    name TEXT NOT NULL,
    display_name TEXT,
    description TEXT,
    parameters TEXT, -- JSON 字符串
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE,
    UNIQUE(device_id, name)
);

-- ============================================================================
-- 标签系统
-- ============================================================================

-- 标签表
CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL CHECK (type IN ('device', 'app')),
    name TEXT NOT NULL,
    description TEXT,
    color TEXT, -- 十六进制颜色代码
    tenant_id TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL,
    UNIQUE(type, name) -- 同一类型下标签名称唯一
);

-- 标签绑定表
CREATE TABLE tag_bindings (
    id TEXT PRIMARY KEY,
    tag_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    target_type TEXT NOT NULL, -- 'device', 'user', 'organization', etc.
    tenant_id TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE,
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL,
    UNIQUE(tag_id, target_id, target_type) -- 防止重复绑定
);

-- ============================================================================
-- 告警系统
-- ============================================================================

-- 告警规则表
CREATE TABLE device_alarm_rules (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    property_id TEXT NOT NULL,
    rule_name TEXT NOT NULL,
    rule_type TEXT NOT NULL, -- 'threshold', 'range', 'change', 'offline'
    condition_config TEXT NOT NULL, -- JSON 字符串，包含条件详情
    alarm_level TEXT NOT NULL CHECK (alarm_level IN ('info', 'warning', 'error', 'critical')),
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    description TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE,
    FOREIGN KEY (property_id) REFERENCES device_properties(id) ON DELETE CASCADE,
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL
);

-- 告警实例表
CREATE TABLE device_alarms (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    property_id TEXT,
    rule_id TEXT,
    alarm_level TEXT NOT NULL CHECK (alarm_level IN ('info', 'warning', 'error', 'critical')),
    alarm_message TEXT NOT NULL,
    alarm_value TEXT, -- 触发告警的值
    threshold_value TEXT, -- 阈值
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
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE,
    FOREIGN KEY (property_id) REFERENCES device_properties(id) ON DELETE SET NULL,
    FOREIGN KEY (rule_id) REFERENCES device_alarm_rules(id) ON DELETE SET NULL,
    FOREIGN KEY (acknowledged_by) REFERENCES users(id) ON DELETE SET NULL,
    FOREIGN KEY (resolved_by) REFERENCES users(id) ON DELETE SET NULL
);

-- ============================================================================
-- 事件和消息系统
-- ============================================================================

-- 系统消息表
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    level INTEGER NOT NULL, -- 1: info, 2: warning, 3: error, 4: critical
    title TEXT NOT NULL,
    content TEXT, -- JSON 字符串
    message_type TEXT,
    device_type TEXT,
    device_id TEXT,
    is_disabled BOOLEAN NOT NULL DEFAULT false,
    confirmor TEXT,
    confirmed_at TEXT,
    confirm_result TEXT,
    child_object TEXT,
    is_false_positive BOOLEAN NOT NULL DEFAULT false,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE SET NULL,
    FOREIGN KEY (confirmor) REFERENCES users(id) ON DELETE SET NULL
);

-- 设备事件触发器表
CREATE TABLE device_event_triggers (
    id TEXT PRIMARY KEY,
    trigger_config TEXT NOT NULL, -- JSON 字符串
    action_type INTEGER NOT NULL,
    target_id TEXT,
    action_args TEXT, -- JSON 字符串
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    action_level INTEGER,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL
);

-- ============================================================================
-- 系统配置
-- ============================================================================

-- 菜单表
CREATE TABLE menus (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    subtitle TEXT,
    path TEXT,
    target TEXT,
    is_divided BOOLEAN NOT NULL DEFAULT false,
    icon TEXT,
    custom_config TEXT, -- JSON 字符串
    header TEXT,
    menu_type TEXT,
    sort_order INTEGER DEFAULT 1,
    parent_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (parent_id) REFERENCES menus(id) ON DELETE CASCADE
);

-- 组件表
CREATE TABLE components (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    version TEXT,
    class_name TEXT,
    device_count INTEGER DEFAULT 0,
    description TEXT,
    options_descriptors TEXT, -- JSON 字符串
    location TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ============================================================================
-- 创建索引以提高查询性能
-- ============================================================================

-- 用户相关索引
CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_parent_id ON users(parent_id);
CREATE INDEX idx_users_is_enabled ON users(is_enabled);
CREATE INDEX idx_user_roles_user_id ON user_roles(user_id);
CREATE INDEX idx_user_roles_role_id ON user_roles(role_id);
CREATE INDEX idx_role_permissions_role_id ON role_permissions(role_id);
CREATE INDEX idx_user_permissions_user_id ON user_permissions(user_id);

-- 设备相关索引
CREATE INDEX idx_devices_name ON devices(name);
CREATE INDEX idx_devices_device_type ON devices(device_type);
CREATE INDEX idx_devices_state ON devices(state);
CREATE INDEX idx_devices_parent_id ON devices(parent_id);
CREATE INDEX idx_devices_product_id ON devices(product_id);
CREATE INDEX idx_devices_organization_id ON devices(organization_id);
CREATE INDEX idx_device_properties_device_id ON device_properties(device_id);
CREATE INDEX idx_device_commands_device_id ON device_commands(device_id);

-- 标签相关索引
CREATE INDEX idx_tags_type ON tags(type);
CREATE INDEX idx_tags_name ON tags(name);
CREATE INDEX idx_tags_tenant_id ON tags(tenant_id);
CREATE INDEX idx_tag_bindings_tag_id ON tag_bindings(tag_id);
CREATE INDEX idx_tag_bindings_target_id ON tag_bindings(target_id);
CREATE INDEX idx_tag_bindings_target_type ON tag_bindings(target_type);
CREATE INDEX idx_tag_bindings_tenant_id ON tag_bindings(tenant_id);

-- 告警相关索引
CREATE INDEX idx_device_alarm_rules_device_id ON device_alarm_rules(device_id);
CREATE INDEX idx_device_alarm_rules_is_enabled ON device_alarm_rules(is_enabled);
CREATE INDEX idx_device_alarms_device_id ON device_alarms(device_id);
CREATE INDEX idx_device_alarms_alarm_level ON device_alarms(alarm_level);
CREATE INDEX idx_device_alarms_alarm_time ON device_alarms(alarm_time);
CREATE INDEX idx_device_alarms_is_acknowledged ON device_alarms(is_acknowledged);
CREATE INDEX idx_device_alarms_is_resolved ON device_alarms(is_resolved);

-- 消息相关索引
CREATE INDEX idx_messages_level ON messages(level);
CREATE INDEX idx_messages_created_at ON messages(created_at);
CREATE INDEX idx_messages_device_id ON messages(device_id);

-- 其他索引
CREATE INDEX idx_organizations_parent_id ON organizations(parent_id);
CREATE INDEX idx_menus_parent_id ON menus(parent_id);
CREATE INDEX idx_menus_sort_order ON menus(sort_order);

-- ============================================================================
-- 插入初始数据
-- ============================================================================

-- 插入默认管理员用户
INSERT INTO users (id, username, password_hash, display_name, is_enabled) VALUES
('admin-user-001', 'admin', 'hashed_admin123', '系统管理员', true);

-- 插入默认角色
INSERT INTO roles (id, name, description, is_administrator) VALUES
('role-admin', '系统管理员', '拥有系统所有权限', true),
('role-operator', '操作员', '设备操作和监控权限', false),
('role-viewer', '查看者', '只读权限', false);

-- 分配管理员角色
INSERT INTO user_roles (id, user_id, role_id) VALUES
('user-role-001', 'admin-user-001', 'role-admin');

-- 插入基础权限
INSERT INTO permissions (id, name, description, resource_type, action) VALUES
('perm-device-read', 'device:read', '查看设备信息', 'device', 'read'),
('perm-device-write', 'device:write', '修改设备信息', 'device', 'write'),
('perm-device-delete', 'device:delete', '删除设备', 'device', 'delete'),
('perm-device-admin', 'device:admin', '设备管理权限', 'device', 'admin'),
('perm-user-read', 'user:read', '查看用户信息', 'user', 'read'),
('perm-user-write', 'user:write', '修改用户信息', 'user', 'write'),
('perm-user-delete', 'user:delete', '删除用户', 'user', 'delete'),
('perm-user-admin', 'user:admin', '用户管理权限', 'user', 'admin'),
('perm-system-admin', 'system:admin', '系统管理权限', 'system', 'admin');

-- 为管理员角色分配所有权限
INSERT INTO role_permissions (id, role_id, permission_id) VALUES
('role-perm-001', 'role-admin', 'perm-device-admin'),
('role-perm-002', 'role-admin', 'perm-user-admin'),
('role-perm-003', 'role-admin', 'perm-system-admin');

-- 为操作员角色分配设备权限
INSERT INTO role_permissions (id, role_id, permission_id) VALUES
('role-perm-004', 'role-operator', 'perm-device-read'),
('role-perm-005', 'role-operator', 'perm-device-write');

-- 为查看者角色分配只读权限
INSERT INTO role_permissions (id, role_id, permission_id) VALUES
('role-perm-006', 'role-viewer', 'perm-device-read'),
('role-perm-007', 'role-viewer', 'perm-user-read');

-- 插入示例产品
INSERT INTO products (id, name, description, version, manufacturer, device_type, protocol_type) VALUES
('product-001', '温湿度传感器', 'DHT22 温湿度传感器', 'v1.0', 'SensorTech', '传感器', 'Modbus RTU'),
('product-002', 'IP摄像头', '高清网络摄像头', 'v2.1', 'CameraCorp', '摄像头', 'ONVIF'),
('product-003', '工业机器人', '六轴工业机器人', 'v3.0', 'RobotInc', '机器人', 'TCP/IP');

-- 插入示例设备
INSERT INTO devices (id, name, display_name, device_type, address, description, driver_name, protocol_type, product_id, state) VALUES
('device-001', 'temp_sensor_01', '车间温度传感器1', '温度传感器', '192.168.1.100:502', '生产车间温度监测', 'simulator', 'Modbus RTU', 'product-001', 1),
('device-002', 'humidity_sensor_01', '车间湿度传感器1', '湿度传感器', '192.168.1.101:502', '生产车间湿度监测', 'simulator', 'Modbus RTU', 'product-001', 1),
('device-003', 'camera_01', '入口监控摄像头', 'IP摄像头', '192.168.1.200', '工厂入口监控', 'simulator', 'ONVIF', 'product-002', 0),
('device-004', 'robot_arm_01', '装配机器人1号', '工业机器人', '192.168.1.50:8080', '产品装配作业', 'simulator', 'TCP/IP', 'product-003', 1),
('device-005', 'camera_02', '车间监控摄像头', 'IP摄像头', '192.168.1.201', '生产车间监控', 'simulator', 'ONVIF', 'product-002', 0);

-- 插入设备属性
INSERT INTO device_properties (id, device_id, name, display_name, description, data_type, unit, min_value, max_value) VALUES
('prop-001', 'device-001', 'temperature', '温度', '当前温度值', 'number', '°C', -40, 125),
('prop-002', 'device-002', 'humidity', '湿度', '当前湿度值', 'number', '%', 0, 100),
('prop-003', 'device-003', 'status', '状态', '摄像头状态', 'string', '', NULL, NULL),
('prop-004', 'device-004', 'position', '位置', '机器人当前位置', 'object', '', NULL, NULL),
('prop-005', 'device-005', 'status', '状态', '摄像头状态', 'string', '', NULL, NULL);

-- 插入设备命令
INSERT INTO device_commands (id, device_id, name, display_name, description, parameters) VALUES
('cmd-001', 'device-003', 'capture', '拍照', '拍摄一张照片', '{}'),
('cmd-002', 'device-004', 'move_to', '移动到位置', '移动到指定位置', '{"x": 0, "y": 0, "z": 0}'),
('cmd-003', 'device-005', 'capture', '拍照', '拍摄一张照片', '{}');

-- 插入标签
INSERT INTO tags (id, type, name, description, color, tenant_id, created_by) VALUES
('tag-device-001', 'device', '温度传感器', '温度监测设备', '#FF6B6B', 'tenant-default-001', 'admin-user-001'),
('tag-device-002', 'device', '湿度传感器', '湿度监测设备', '#4ECDC4', 'tenant-default-001', 'admin-user-001'),
('tag-device-003', 'device', '摄像头', '视频监控设备', '#45B7D1', 'tenant-default-001', 'admin-user-001'),
('tag-device-004', 'device', '机器人', '自动化设备', '#96CEB4', 'tenant-default-001', 'admin-user-001'),
('tag-device-005', 'device', '在线设备', '当前在线的设备', '#FFEAA7', 'tenant-default-001', 'admin-user-001'),
('tag-device-006', 'device', '离线设备', '当前离线的设备', '#DDA0DD', 'tenant-default-001', 'admin-user-001'),
('tag-device-007', 'device', '生产设备', '生产相关设备', '#98D8C8', 'tenant-default-001', 'admin-user-001'),
('tag-device-008', 'device', '监控设备', '监控相关设备', '#F7DC6F', 'tenant-default-001', 'admin-user-001'),
('tag-app-001', 'app', '生产环境', '生产环境应用', '#52C41A', 'tenant-default-001', 'admin-user-001'),
('tag-app-002', 'app', '测试环境', '测试环境应用', '#1890FF', 'tenant-default-001', 'admin-user-001'),
('tag-app-003', 'app', '开发环境', '开发环境应用', '#722ED1', 'tenant-default-001', 'admin-user-001');

-- 绑定设备标签
INSERT INTO tag_bindings (id, tag_id, target_id, target_type, tenant_id, created_by) VALUES
('binding-001', 'tag-device-001', 'device-001', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-002', 'tag-device-005', 'device-001', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-003', 'tag-device-007', 'device-001', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-004', 'tag-device-002', 'device-002', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-005', 'tag-device-005', 'device-002', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-006', 'tag-device-007', 'device-002', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-007', 'tag-device-003', 'device-003', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-008', 'tag-device-006', 'device-003', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-009', 'tag-device-008', 'device-003', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-010', 'tag-device-004', 'device-004', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-011', 'tag-device-005', 'device-004', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-012', 'tag-device-007', 'device-004', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-013', 'tag-device-003', 'device-005', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-014', 'tag-device-006', 'device-005', 'device', 'tenant-default-001', 'admin-user-001'),
('binding-015', 'tag-device-008', 'device-005', 'device', 'tenant-default-001', 'admin-user-001');

-- 插入告警规则
INSERT INTO device_alarm_rules (id, device_id, property_id, rule_name, rule_type, condition_config, alarm_level, created_by) VALUES
('alarm-rule-001', 'device-001', 'prop-001', '高温告警', 'threshold', '{"operator": "gt", "value": 80}', 'warning', 'admin-user-001'),
('alarm-rule-002', 'device-001', 'prop-001', '超高温告警', 'threshold', '{"operator": "gt", "value": 100}', 'critical', 'admin-user-001'),
('alarm-rule-003', 'device-002', 'prop-002', '高湿度告警', 'threshold', '{"operator": "gt", "value": 90}', 'warning', 'admin-user-001');

-- 插入示例告警
INSERT INTO device_alarms (id, device_id, property_id, rule_id, alarm_level, alarm_message, alarm_value, threshold_value, alarm_time) VALUES
('alarm-001', 'device-001', 'prop-001', 'alarm-rule-001', 'warning', '温度超过警告阈值', '85', '80', datetime('now', '-2 hours')),
('alarm-002', 'device-002', 'prop-002', 'alarm-rule-003', 'warning', '湿度超过警告阈值', '95', '90', datetime('now', '-1 hour'));

PRAGMA foreign_keys = OFF;