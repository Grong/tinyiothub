-- Create event security tables
-- This migration adds tables for event audit logging and security features

-- Enable foreign keys
PRAGMA foreign_keys = ON;

-- Event audit logs table
CREATE TABLE IF NOT EXISTS event_audit_logs (
    id TEXT PRIMARY KEY,
    log_type TEXT NOT NULL CHECK (log_type IN ('access', 'creation', 'modification', 'deletion')),
    user_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT,
    event_level INTEGER,
    action TEXT NOT NULL,
    result TEXT NOT NULL CHECK (result IN ('allowed', 'denied', 'error', 'success')),
    details TEXT, -- JSON string with additional information
    ip_address TEXT,
    user_agent TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Indexes for audit logs table
CREATE INDEX IF NOT EXISTS idx_audit_user_id ON event_audit_logs(user_id);
CREATE INDEX IF NOT EXISTS idx_audit_event_id ON event_audit_logs(event_id);
CREATE INDEX IF NOT EXISTS idx_audit_created_at ON event_audit_logs(created_at);
CREATE INDEX IF NOT EXISTS idx_audit_log_type ON event_audit_logs(log_type);
CREATE INDEX IF NOT EXISTS idx_audit_result ON event_audit_logs(result);
CREATE INDEX IF NOT EXISTS idx_audit_action ON event_audit_logs(action);

-- Event encrypted content table (for storing encrypted event content separately)
CREATE TABLE IF NOT EXISTS event_encrypted_content (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    encrypted_data TEXT NOT NULL, -- Base64 encoded encrypted content
    nonce TEXT NOT NULL, -- Base64 encoded nonce
    algorithm TEXT NOT NULL DEFAULT 'AES-256-GCM',
    content_hash TEXT NOT NULL, -- SHA-256 hash for integrity verification
    encrypted_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (event_id) REFERENCES events(id) ON DELETE CASCADE
);

-- Index for encrypted content table
CREATE INDEX IF NOT EXISTS idx_encrypted_content_event_id ON event_encrypted_content(event_id);

-- Event security settings table (for per-event security configuration)
CREATE TABLE IF NOT EXISTS event_security_settings (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL, -- 'system.user_auth', 'device.connection', etc.
    min_role_level INTEGER NOT NULL DEFAULT 1, -- Minimum role level required (1=user, 2=operator, 3=admin)
    require_encryption BOOLEAN NOT NULL DEFAULT false,
    audit_level TEXT NOT NULL DEFAULT 'normal' CHECK (audit_level IN ('none', 'basic', 'normal', 'detailed')),
    retention_days INTEGER NOT NULL DEFAULT 90,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(event_type)
);

-- Insert default security settings for different event types
INSERT OR IGNORE INTO event_security_settings (id, event_type, min_role_level, require_encryption, audit_level) VALUES
('sec-001', 'system.user_auth', 1, true, 'detailed'),
('sec-002', 'system.user_operation', 1, false, 'normal'),
('sec-003', 'system.system_config', 2, true, 'detailed'),
('sec-004', 'system.system_error', 1, false, 'normal'),
('sec-005', 'device.connection', 1, false, 'basic'),
('sec-006', 'device.property', 1, false, 'basic'),
('sec-007', 'device.command', 2, false, 'normal'),
('sec-008', 'device.business', 1, false, 'basic');

-- Add event permissions to the existing permissions table
INSERT OR IGNORE INTO permissions (id, name, description, resource_type, action) VALUES
('perm-event-read', 'event:read', '查看事件信息', 'event', 'read'),
('perm-event-create', 'event:create', '创建事件', 'event', 'create'),
('perm-event-update', 'event:update', '修改事件信息', 'event', 'update'),
('perm-event-delete', 'event:delete', '删除事件', 'event', 'delete'),
('perm-event-admin', 'event:admin', '事件管理权限', 'event', 'admin'),
('perm-event-audit', 'event:audit', '查看事件审计日志', 'event', 'audit');

-- Assign event permissions to existing roles
-- Admin role gets all event permissions
INSERT OR IGNORE INTO role_permissions (id, role_id, permission_id) VALUES
('role-perm-event-001', 'role-admin', 'perm-event-admin'),
('role-perm-event-002', 'role-admin', 'perm-event-audit');

-- Operator role gets read, create, update permissions
INSERT OR IGNORE INTO role_permissions (id, role_id, permission_id) VALUES
('role-perm-event-003', 'role-operator', 'perm-event-read'),
('role-perm-event-004', 'role-operator', 'perm-event-create'),
('role-perm-event-005', 'role-operator', 'perm-event-update');

-- Viewer role gets only read permission
INSERT OR IGNORE INTO role_permissions (id, role_id, permission_id) VALUES
('role-perm-event-006', 'role-viewer', 'perm-event-read');

-- Create a view for easy access to user event permissions
CREATE VIEW IF NOT EXISTS user_event_permissions AS
SELECT DISTINCT
    u.id as user_id,
    u.username,
    p.resource_type,
    p.action,
    p.name as permission_name,
    'role' as grant_type,
    r.name as role_name
FROM users u
JOIN user_roles ur ON u.id = ur.user_id
JOIN roles r ON ur.role_id = r.id
JOIN role_permissions rp ON r.id = rp.role_id
JOIN permissions p ON rp.permission_id = p.id
WHERE p.resource_type = 'event'

UNION

SELECT DISTINCT
    u.id as user_id,
    u.username,
    p.resource_type,
    p.action,
    p.name as permission_name,
    'direct' as grant_type,
    NULL as role_name
FROM users u
JOIN user_permissions up ON u.id = up.user_id
JOIN permissions p ON up.permission_id = p.id
WHERE p.resource_type = 'event'
AND (up.expires_at IS NULL OR up.expires_at > datetime('now'));

-- Disable foreign keys
PRAGMA foreign_keys = OFF;