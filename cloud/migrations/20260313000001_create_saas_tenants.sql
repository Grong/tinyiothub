-- TinyIoTHub SaaS 平台 - 租户与订阅模块
-- 创建时间: 2026-03-12

PRAGMA foreign_keys = ON;

-- ============================================================================
-- 订阅计划表
-- ============================================================================
CREATE TABLE IF NOT EXISTS subscription_plans (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,                -- 计划名 (free, basic, pro, enterprise)
    display_name TEXT NOT NULL,        -- 显示名称
    description TEXT,                  -- 描述
    
    -- 配额限制
    device_limit INTEGER NOT NULL DEFAULT 0,      -- 设备数量限制 (0=无限制)
    api_call_limit INTEGER NOT NULL DEFAULT 0,    -- API 调用限额 (0=无限制)
    storage_mb INTEGER NOT NULL DEFAULT 0,        -- 存储空间 MB (0=无限制)
    user_limit INTEGER NOT NULL DEFAULT 0,        -- 用户数量限制 (0=无限制)
    
    -- 价格
    price_monthly REAL NOT NULL DEFAULT 0,        -- 月付价格
    price_yearly REAL NOT NULL DEFAULT 0,         -- 年付价格
    
    -- 功能开关 (JSON)
    features TEXT NOT NULL DEFAULT '{}',          -- {"webhook": true, "sms": true, ...}
    
    -- 排序
    sort_order INTEGER NOT NULL DEFAULT 0,
    
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 默认订阅计划
INSERT INTO subscription_plans (id, name, display_name, description, device_limit, api_call_limit, storage_mb, user_limit, price_monthly, price_yearly, features, sort_order) VALUES
    ('plan_free', 'free', '免费版', '适合个人测试', 10, 1000, 100, 2, 0, 0, '{"device_group": false, "webhook": true, "sms": false, "email": false, "api_access": true, "custom_brand": false}', 1),
    ('plan_basic', 'basic', '基础版', '适合小型项目', 100, 10000, 1024, 5, 99, 990, '{"device_group": true, "webhook": true, "sms": true, "email": true, "api_access": true, "custom_brand": false}', 2),
    ('plan_pro', 'pro', '专业版', '适合中大型项目', 1000, 100000, 10240, 20, 399, 3990, '{"device_group": true, "webhook": true, "sms": true, "email": true, "api_access": true, "custom_brand": true}', 3),
    ('plan_enterprise', 'enterprise', '企业版', '适合大型企业', 0, 0, 0, 0, 0, 0, '{"device_group": true, "webhook": true, "sms": true, "email": true, "api_access": true, "custom_brand": true, "dedicated_support": true, "sla": true}', 4);

-- ============================================================================
-- 租户表
-- ============================================================================
CREATE TABLE IF NOT EXISTS tenants (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,                -- 租户名称
    slug TEXT NOT NULL UNIQUE,         -- 租户标识 (用于子域名)
    
    -- 状态
    status TEXT NOT NULL DEFAULT 'active',  -- active, suspended, trial, inactive
    
    -- 订阅
    plan_id TEXT NOT NULL DEFAULT 'plan_free',
    subscription_status TEXT NOT NULL DEFAULT 'active',  -- active, canceled, past_due
    trial_expires_at TEXT,             -- 试用过期时间
    
    -- 计费
    billing_email TEXT,                -- 计费邮箱
    billing_contact TEXT,              --  billing联系人
    
    -- 设置
    timezone TEXT NOT NULL DEFAULT 'Asia/Shanghai',
    locale TEXT NOT NULL DEFAULT 'zh-CN',
    
    -- 品牌定制
    custom_logo TEXT,                  -- 自定义 logo URL
    custom_theme TEXT,                 -- 自定义主题 JSON
    
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (plan_id) REFERENCES subscription_plans(id)
);

CREATE INDEX IF NOT EXISTS idx_tenants_slug ON tenants(slug);
CREATE INDEX IF NOT EXISTS idx_tenants_status ON tenants(status);

-- ============================================================================
-- 租户用户表 (租户下的用户)
-- ============================================================================
CREATE TABLE IF NOT EXISTS tenant_users (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    user_id TEXT NOT NULL,             -- 关联主系统用户
    
    role TEXT NOT NULL DEFAULT 'member',  -- owner, admin, member, viewer
    
    invitation_status TEXT NOT NULL DEFAULT 'accepted',  -- pending, accepted
    invited_by TEXT,
    invited_at TEXT,
    joined_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    UNIQUE(tenant_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_tenant_users_tenant ON tenant_users(tenant_id);
CREATE INDEX IF NOT EXISTS idx_tenant_users_user ON tenant_users(user_id);

-- ============================================================================
-- API Keys 表
-- ============================================================================
CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    
    name TEXT NOT NULL,                -- 密钥名称
    key_hash TEXT NOT NULL,           -- 密钥 hash (SHA256)
    prefix TEXT NOT NULL,             -- 前缀 (sk_live_xxx)
    
    -- 权限
    permissions TEXT NOT NULL DEFAULT '["read"]',  -- JSON array: ["read", "write"]
    
    -- 限流
    rate_limit INTEGER NOT NULL DEFAULT 60,  -- 每分钟请求数
    
    -- 状态
    is_enabled INTEGER NOT NULL DEFAULT 1,
    is_revoked INTEGER NOT NULL DEFAULT 0,
    
    -- 使用统计
    last_used_at TEXT,
    last_used_ip TEXT,
    request_count INTEGER NOT NULL DEFAULT 0,
    
    -- 过期时间
    expires_at TEXT,
    
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_api_keys_tenant ON api_keys(tenant_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_prefix ON api_keys(prefix);

-- ============================================================================
-- API 使用日志表
-- ============================================================================
CREATE TABLE IF NOT EXISTS api_usage (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    api_key_id TEXT,
    
    -- 请求信息
    method TEXT NOT NULL,
    path TEXT NOT NULL,
    query_params TEXT,
    
    -- 响应信息
    status_code INTEGER NOT NULL,
    response_size INTEGER,
    
    -- 性能
    latency_ms INTEGER NOT NULL,
    
    -- 客户端信息
    ip_address TEXT,
    user_agent TEXT,
    
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (api_key_id) REFERENCES api_keys(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_api_usage_tenant ON api_usage(tenant_id, created_at);
CREATE INDEX IF NOT EXISTS idx_api_usage_key ON api_usage(api_key_id, created_at);

-- ============================================================================
-- 订阅支付记录表
-- ============================================================================
CREATE TABLE IF NOT EXISTS subscription_payments (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    plan_id TEXT NOT NULL,
    
    -- 金额
    amount REAL NOT NULL,             -- 支付金额
    currency TEXT NOT NULL DEFAULT 'CNY',
    
    -- 支付方式
    payment_method TEXT,               -- alipay, wechat, stripe, bank_transfer
    transaction_id TEXT,               -- 第三方交易号
    
    -- 状态
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, processing, succeeded, failed, refunded
    
    -- 周期
    period_start TEXT,                 -- 订阅开始时间
    period_end TEXT,                   -- 订阅结束时间
    
    -- 备注
    description TEXT,
    metadata TEXT,                     -- JSON 扩展字段
    
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    paid_at TEXT,
    
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (plan_id) REFERENCES subscription_plans(id)
);

CREATE INDEX IF NOT EXISTS idx_subscription_payments_tenant ON subscription_payments(tenant_id);
CREATE INDEX IF NOT EXISTS idx_subscription_payments_status ON subscription_payments(status);

-- ============================================================================
-- 租户配额使用表 (用于实时检查限额)
-- ============================================================================
CREATE TABLE IF NOT EXISTS tenant_usage (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL UNIQUE,
    
    -- 当前使用量
    device_count INTEGER NOT NULL DEFAULT 0,
    api_call_count INTEGER NOT NULL DEFAULT 0,
    api_call_reset_at TEXT,            -- API 调用计数重置时间
    
    storage_used_bytes INTEGER NOT NULL DEFAULT 0,
    user_count INTEGER NOT NULL DEFAULT 0,
    
    -- 本月统计
    total_api_calls INTEGER NOT NULL DEFAULT 0,
    total_api_errors INTEGER NOT NULL DEFAULT 0,
    
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_tenant_usage_tenant ON tenant_usage(tenant_id);
