-- Create default tenant and workspace for the admin user
-- This fixes the FK constraint failure when admin tries to create workspaces

-- 1. Create a default tenant (only if no tenants exist)
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

-- 2. Link admin user to default tenant as owner (only if admin has no tenant)
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

-- 3. Create default workspace for the default tenant
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

-- 4. Assign existing devices (without tenant_id) to the default tenant
UPDATE devices
SET tenant_id = 'tenant-default-001'
WHERE tenant_id IS NULL
  AND EXISTS (SELECT 1 FROM tenants WHERE id = 'tenant-default-001');

-- 5. Assign existing devices (without workspace_id) to the default workspace
UPDATE devices
SET workspace_id = 'ws-default-001'
WHERE workspace_id IS NULL
  AND tenant_id = 'tenant-default-001'
  AND EXISTS (SELECT 1 FROM workspaces WHERE id = 'ws-default-001');
