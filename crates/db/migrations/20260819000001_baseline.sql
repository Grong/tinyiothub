-- TinyIoTHub baseline schema (squashed from 68 historical migrations).
-- Source: terminal state of the old migration chain, exported 2026-08-19.
-- Verification: crates/db/tests/baseline_schema_tests.rs diffs a database built
-- solely from this file against the old-chain reference DB (TIH_OLDCHAIN_DB);
-- the (type, name, normalized sql) sets must be identical.
-- Pure DDL only: seed data moved to seed_system (Task 3).
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
    last_login_at TEXT, phone_number VARCHAR(20),
    FOREIGN KEY (parent_id) REFERENCES users(id) ON DELETE SET NULL
);
CREATE TABLE roles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    is_administrator BOOLEAN NOT NULL DEFAULT false,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
, workspace_id TEXT);
CREATE TABLE user_roles (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    role_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE,
    UNIQUE(user_id, role_id)
);
CREATE TABLE permissions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    resource_type TEXT NOT NULL, -- 'device', 'user', 'system', etc.
    action TEXT NOT NULL, -- 'read', 'write', 'delete', 'admin'
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
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
CREATE TABLE organizations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    parent_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (parent_id) REFERENCES organizations(id) ON DELETE SET NULL
);
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
CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_parent_id ON users(parent_id);
CREATE INDEX idx_users_is_enabled ON users(is_enabled);
CREATE INDEX idx_user_roles_user_id ON user_roles(user_id);
CREATE INDEX idx_user_roles_role_id ON user_roles(role_id);
CREATE INDEX idx_role_permissions_role_id ON role_permissions(role_id);
CREATE INDEX idx_user_permissions_user_id ON user_permissions(user_id);
CREATE INDEX idx_tag_bindings_tag_id ON tag_bindings(tag_id);
CREATE INDEX idx_tag_bindings_target_id ON tag_bindings(target_id);
CREATE INDEX idx_tag_bindings_target_type ON tag_bindings(target_type);
CREATE INDEX idx_tag_bindings_tenant_id ON tag_bindings(tenant_id);
CREATE INDEX idx_messages_level ON messages(level);
CREATE INDEX idx_messages_created_at ON messages(created_at);
CREATE INDEX idx_messages_device_id ON messages(device_id);
CREATE INDEX idx_organizations_parent_id ON organizations(parent_id);
CREATE INDEX idx_menus_parent_id ON menus(parent_id);
CREATE INDEX idx_menus_sort_order ON menus(sort_order);
CREATE TABLE template_categories (
    name TEXT PRIMARY KEY,
    display_name TEXT NOT NULL, -- JSON格式的多语言显示名称
    description TEXT, -- JSON格式的多语言描述
    sort_order INTEGER DEFAULT 0,
    is_active INTEGER DEFAULT 1,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_template_categories_sort_order ON template_categories(sort_order);
CREATE INDEX idx_template_categories_is_active ON template_categories(is_active);
CREATE TABLE device_traces (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    trace_type TEXT NOT NULL,        -- 追踪类型: operation, status_change, error, warning, info
    level TEXT NOT NULL,             -- 日志级别: debug, info, warn, error, critical
    category TEXT NOT NULL,          -- 分类: system, user, device, network, performance
    title TEXT NOT NULL,             -- 标题
    message TEXT NOT NULL,           -- 详细消息
    details TEXT,                    -- JSON 格式的详细信息
    source TEXT,                     -- 来源: api, system, device, scheduler
    user_id TEXT,                    -- 操作用户ID（如果适用）
    session_id TEXT,                 -- 会话ID（如果适用）
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    -- 外键约束
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE
);
CREATE INDEX idx_device_traces_device_id ON device_traces(device_id);
CREATE INDEX idx_device_traces_trace_type ON device_traces(trace_type);
CREATE INDEX idx_device_traces_level ON device_traces(level);
CREATE INDEX idx_device_traces_category ON device_traces(category);
CREATE INDEX idx_device_traces_created_at ON device_traces(created_at);
CREATE INDEX idx_device_traces_user_id ON device_traces(user_id);
CREATE INDEX idx_device_traces_source ON device_traces(source);
CREATE INDEX idx_device_traces_device_time ON device_traces(device_id, created_at DESC);
CREATE INDEX idx_device_traces_device_type ON device_traces(device_id, trace_type);
CREATE INDEX idx_device_traces_device_level ON device_traces(device_id, level);
CREATE TABLE events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL, -- 'system' or 'device'
    event_subtype TEXT NOT NULL, -- specific subtype like 'user_auth', 'connection', etc.
    event_level INTEGER NOT NULL, -- 1: debug, 2: info, 3: warning, 4: error, 5: critical
    timestamp TEXT NOT NULL, -- ISO 8601 format
    source_type TEXT NOT NULL, -- 'system', 'device', 'user'
    source_id TEXT, -- identifier of the source
    title TEXT NOT NULL,
    content TEXT, -- JSON format rich content
    metadata TEXT, -- JSON format additional metadata
    user_id TEXT, -- user who triggered the event (if applicable)
    device_id TEXT, -- device related to the event (if applicable)
    property_id TEXT, -- device property related to the event (if applicable)
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
, occurrence_count INTEGER DEFAULT 1, acknowledged BOOLEAN DEFAULT 0, acknowledged_by TEXT, acknowledged_at TEXT, workspace_id TEXT NOT NULL DEFAULT '', is_status INTEGER NOT NULL DEFAULT 0, actor TEXT NOT NULL DEFAULT 'device');
CREATE INDEX idx_events_timestamp ON events (timestamp);
CREATE INDEX idx_events_level ON events (event_level);
CREATE INDEX idx_events_type ON events (event_type, event_subtype);
CREATE INDEX idx_events_device ON events (device_id);
CREATE INDEX idx_events_user ON events (user_id);
CREATE INDEX idx_events_source ON events (source_type, source_id);
CREATE INDEX idx_events_created ON events (created_at);
CREATE TABLE notification_rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    event_type TEXT, -- filter by event type (optional)
    event_subtype TEXT, -- filter by event subtype (optional)
    event_level INTEGER, -- minimum event level to trigger (optional)
    device_filter TEXT, -- JSON format device filter conditions (optional)
    notification_methods TEXT NOT NULL, -- JSON array: ["websocket", "email", "sms"]
    recipients TEXT NOT NULL, -- JSON array: recipient list
    enabled BOOLEAN DEFAULT TRUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
, workspace_id TEXT);
CREATE INDEX idx_notification_rules_enabled ON notification_rules (enabled);
CREATE INDEX idx_notification_rules_type ON notification_rules (event_type, event_subtype);
CREATE INDEX idx_notification_rules_level ON notification_rules (event_level);
CREATE TABLE notification_history (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    notification_method TEXT NOT NULL, -- 'websocket', 'email', 'sms'
    recipient TEXT NOT NULL,
    status TEXT NOT NULL, -- 'pending', 'sent', 'failed'
    sent_at TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
, workspace_id TEXT);
CREATE INDEX idx_notification_history_event ON notification_history (event_id);
CREATE INDEX idx_notification_history_rule ON notification_history (rule_id);
CREATE INDEX idx_notification_history_status ON notification_history (status);
CREATE INDEX idx_notification_history_created ON notification_history (created_at);
CREATE VIEW event_statistics AS
SELECT 
    event_level,
    event_type,
    event_subtype,
    COUNT(*) as count,
    DATE(timestamp) as event_date
FROM events 
WHERE timestamp >= datetime('now', '-30 days')
GROUP BY event_level, event_type, event_subtype, DATE(timestamp);
CREATE TRIGGER update_notification_rules_timestamp
    AFTER UPDATE ON notification_rules
    FOR EACH ROW
BEGIN
    UPDATE notification_rules 
    SET updated_at = datetime('now')
    WHERE id = NEW.id;
END;
CREATE TRIGGER cleanup_old_notification_history
    AFTER INSERT ON notification_history
    FOR EACH ROW
BEGIN
    DELETE FROM notification_history 
    WHERE created_at < datetime('now', '-30 days');
END;
CREATE TABLE event_audit_logs (
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
CREATE INDEX idx_audit_user_id ON event_audit_logs(user_id);
CREATE INDEX idx_audit_event_id ON event_audit_logs(event_id);
CREATE INDEX idx_audit_created_at ON event_audit_logs(created_at);
CREATE INDEX idx_audit_log_type ON event_audit_logs(log_type);
CREATE INDEX idx_audit_result ON event_audit_logs(result);
CREATE INDEX idx_audit_action ON event_audit_logs(action);
CREATE TABLE event_encrypted_content (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    encrypted_data TEXT NOT NULL, -- Base64 encoded encrypted content
    nonce TEXT NOT NULL, -- Base64 encoded nonce
    algorithm TEXT NOT NULL DEFAULT 'AES-256-GCM',
    content_hash TEXT NOT NULL, -- SHA-256 hash for integrity verification
    encrypted_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (event_id) REFERENCES events(id) ON DELETE CASCADE
);
CREATE INDEX idx_encrypted_content_event_id ON event_encrypted_content(event_id);
CREATE TABLE event_security_settings (
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
CREATE VIEW user_event_permissions AS
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
CREATE INDEX idx_events_timestamp_level_device ON events (timestamp, event_level, device_id) WHERE device_id IS NOT NULL;
CREATE INDEX idx_events_timestamp_type_subtype ON events (timestamp, event_type, event_subtype);
CREATE INDEX idx_events_level_created ON events (event_level, created_at);
CREATE INDEX idx_events_source_timestamp ON events (source_type, source_id, timestamp);
CREATE INDEX idx_events_critical ON events (timestamp) WHERE event_level >= 4;
CREATE INDEX idx_events_device_timestamp ON events (device_id, timestamp) WHERE device_id IS NOT NULL;
CREATE INDEX idx_events_user_timestamp ON events (user_id, timestamp) WHERE user_id IS NOT NULL;
CREATE INDEX idx_notification_rules_enabled_level ON notification_rules (enabled, event_level) WHERE enabled = 1;
CREATE INDEX idx_notification_history_status_method ON notification_history (status, notification_method, created_at);
CREATE TABLE event_performance_alerts (
    id TEXT PRIMARY KEY,
    alert_type TEXT NOT NULL, -- 'high_processing_time', 'high_queue_size', etc.
    severity TEXT NOT NULL, -- 'info', 'warning', 'critical'
    message TEXT NOT NULL,
    current_value REAL NOT NULL,
    threshold_value REAL NOT NULL,
    resolved BOOLEAN DEFAULT FALSE,
    resolved_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_performance_alerts_type_severity ON event_performance_alerts (alert_type, severity);
CREATE INDEX idx_performance_alerts_unresolved ON event_performance_alerts (resolved, created_at) WHERE resolved = 0;
CREATE INDEX idx_performance_alerts_created ON event_performance_alerts (created_at);
CREATE TABLE event_optimization_history (
    id TEXT PRIMARY KEY,
    optimization_type TEXT NOT NULL, -- 'index_creation', 'vacuum', 'analyze', etc.
    description TEXT NOT NULL,
    execution_time_ms REAL,
    rows_affected INTEGER,
    success BOOLEAN DEFAULT TRUE,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_optimization_history_type ON event_optimization_history (optimization_type, created_at);
CREATE INDEX idx_optimization_history_created ON event_optimization_history (created_at);
CREATE TABLE event_load_balancer_stats (
    id TEXT PRIMARY KEY,
    worker_count INTEGER NOT NULL,
    active_workers INTEGER NOT NULL,
    queue_size INTEGER NOT NULL,
    total_processed INTEGER NOT NULL,
    total_errors INTEGER NOT NULL,
    success_rate REAL NOT NULL,
    throughput_per_second REAL NOT NULL,
    backpressure_active BOOLEAN DEFAULT FALSE,
    timestamp TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_load_balancer_stats_timestamp ON event_load_balancer_stats (timestamp);
CREATE INDEX idx_load_balancer_stats_created ON event_load_balancer_stats (created_at);
CREATE TABLE event_query_performance (
    id TEXT PRIMARY KEY,
    query_name TEXT NOT NULL,
    query_type TEXT NOT NULL, -- 'select', 'insert', 'update', 'delete'
    execution_time_ms REAL NOT NULL,
    rows_affected INTEGER,
    success BOOLEAN DEFAULT TRUE,
    error_message TEXT,
    timestamp TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_query_performance_name_timestamp ON event_query_performance (query_name, timestamp);
CREATE INDEX idx_query_performance_slow ON event_query_performance (execution_time_ms, timestamp) WHERE execution_time_ms > 100;
CREATE INDEX idx_query_performance_created ON event_query_performance (created_at);
CREATE VIEW event_performance_summary AS
SELECT 
    'events_per_hour' as metric_name,
    COUNT(*) as metric_value,
    strftime('%Y-%m-%d %H:00:00', timestamp) as time_bucket
FROM events 
WHERE timestamp > datetime('now', '-24 hours')
GROUP BY strftime('%Y-%m-%d %H:00:00', timestamp)
UNION ALL
SELECT 
    'errors_per_hour' as metric_name,
    COUNT(*) as metric_value,
    strftime('%Y-%m-%d %H:00:00', timestamp) as time_bucket
FROM events 
WHERE timestamp > datetime('now', '-24 hours') AND event_level >= 4
GROUP BY strftime('%Y-%m-%d %H:00:00', timestamp);
CREATE TRIGGER cleanup_old_performance_alerts
    AFTER INSERT ON event_performance_alerts
    FOR EACH ROW
    WHEN (SELECT COUNT(*) FROM event_performance_alerts WHERE resolved = 1) > 1000
BEGIN
    DELETE FROM event_performance_alerts 
    WHERE resolved = 1 AND created_at < datetime('now', '-30 days');
END;
CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    job_type TEXT NOT NULL DEFAULT 'http', -- http, script, device_command, sql
    cron_expression TEXT NOT NULL,
    
    -- 任务配置 (JSON)
    config TEXT NOT NULL DEFAULT '{}', -- 根据 job_type 不同存储不同配置
    
    -- 执行配置
    timeout_seconds INTEGER DEFAULT 300,
    retry_count INTEGER DEFAULT 0,
    retry_delay_seconds INTEGER DEFAULT 60,
    concurrency INTEGER DEFAULT 1, -- 同时运行实例数
    
    -- 目标配置
    target_device_id TEXT, -- 可选的关联设备
    target_command_name TEXT, -- 设备命令名称
    target_command_params TEXT, -- 设备命令参数 (JSON)
    
    -- 状态
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    is_running BOOLEAN NOT NULL DEFAULT false,
    
    -- 统计
    last_run_at TEXT,
    last_run_status TEXT, -- success, failed, timeout
    last_run_error TEXT,
    next_run_at TEXT,
    run_count INTEGER DEFAULT 0,
    success_count INTEGER DEFAULT 0,
    fail_count INTEGER DEFAULT 0,
    
    -- 标签 (JSON array)
    tags TEXT DEFAULT '[]',
    
    -- 告警配置 (JSON)
    alert_config TEXT DEFAULT '{"on_failure": false, "on_timeout": true}',
    
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    created_by TEXT,
    FOREIGN KEY (target_device_id) REFERENCES devices(id) ON DELETE SET NULL
);
CREATE TABLE job_executions (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    
    -- 执行信息
    started_at TEXT NOT NULL,
    ended_at TEXT,
    duration_ms INTEGER,
    status TEXT NOT NULL, -- pending, running, success, failed, timeout, cancelled
    
    -- 执行结果
    result TEXT, -- 执行结果内容
    error_message TEXT,
    error_trace TEXT,
    
    -- 触发信息
    trigger_type TEXT NOT NULL DEFAULT 'schedule', -- schedule, manual, api
    triggered_by TEXT,
    
    -- 运行时信息
    worker_id TEXT,
    memory_usage_bytes INTEGER,
    cpu_time_ms INTEGER,
    
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE
);
CREATE TABLE job_dependencies (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    depends_on_job_id TEXT NOT NULL,
    condition TEXT NOT NULL DEFAULT 'success', -- success, failure, always, never
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE,
    FOREIGN KEY (depends_on_job_id) REFERENCES jobs(id) ON DELETE CASCADE,
    UNIQUE(job_id, depends_on_job_id)
);
CREATE TABLE job_logs (
    id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    
    level TEXT NOT NULL, -- debug, info, warn, error
    message TEXT NOT NULL,
    details TEXT, -- JSON
    
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (execution_id) REFERENCES job_executions(id) ON DELETE CASCADE,
    FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE
);
CREATE INDEX idx_jobs_enabled ON jobs(is_enabled);
CREATE INDEX idx_jobs_next_run ON jobs(next_run_at);
CREATE INDEX idx_jobs_type ON jobs(job_type);
CREATE INDEX idx_job_executions_job_id ON job_executions(job_id);
CREATE INDEX idx_job_executions_status ON job_executions(status);
CREATE INDEX idx_job_executions_started ON job_executions(started_at);
CREATE INDEX idx_job_logs_execution ON job_logs(execution_id);
CREATE INDEX idx_job_logs_timestamp ON job_logs(timestamp);
CREATE TABLE notification_channels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    channel_type TEXT NOT NULL, -- sms, email, webhook
    
    -- 渠道配置 (JSON)
    config TEXT NOT NULL DEFAULT '{}',
    
    -- 状态
    is_enabled INTEGER NOT NULL DEFAULT 1,
    
    -- 元数据
    description TEXT,
    
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
, workspace_id TEXT);
CREATE INDEX idx_notification_channels_type ON notification_channels(channel_type);
CREATE INDEX idx_notification_channels_enabled ON notification_channels(is_enabled);
CREATE TABLE subscription_plans (
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
CREATE TABLE tenants (
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
CREATE INDEX idx_tenants_slug ON tenants(slug);
CREATE INDEX idx_tenants_status ON tenants(status);
CREATE TABLE tenant_users (
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
CREATE INDEX idx_tenant_users_tenant ON tenant_users(tenant_id);
CREATE INDEX idx_tenant_users_user ON tenant_users(user_id);
CREATE TABLE api_usage (
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
CREATE INDEX idx_api_usage_tenant ON api_usage(tenant_id, created_at);
CREATE INDEX idx_api_usage_key ON api_usage(api_key_id, created_at);
CREATE TABLE subscription_payments (
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
CREATE INDEX idx_subscription_payments_tenant ON subscription_payments(tenant_id);
CREATE INDEX idx_subscription_payments_status ON subscription_payments(status);
CREATE TABLE tenant_usage (
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
CREATE INDEX idx_tenant_usage_tenant ON tenant_usage(tenant_id);
CREATE TABLE sms_codes (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    phone VARCHAR(20) NOT NULL,
    code VARCHAR(10) NOT NULL,
    purpose VARCHAR(20) NOT NULL DEFAULT 'login', -- login, register, reset_password
    expires_at TIMESTAMP NOT NULL,
    verified_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    ip_address VARCHAR(45),
    user_agent TEXT
, updated_at TEXT);
CREATE INDEX idx_sms_codes_phone ON sms_codes(phone);
CREATE INDEX idx_sms_codes_code ON sms_codes(code);
CREATE INDEX idx_sms_codes_expires ON sms_codes(expires_at);
CREATE TABLE social_bindings (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    user_id TEXT NOT NULL,
    tenant_id TEXT,
    provider VARCHAR(20) NOT NULL, -- wechat, wechat_miniprogram
    provider_user_id VARCHAR(100) NOT NULL,
    union_id VARCHAR(100),
    nickname VARCHAR(100),
    avatar_url TEXT,
    access_token TEXT,
    refresh_token TEXT,
    expires_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(provider, provider_user_id),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);
CREATE INDEX idx_social_bindings_user ON social_bindings(user_id);
CREATE INDEX idx_social_bindings_provider ON social_bindings(provider, provider_user_id);
CREATE TABLE social_configs (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    provider VARCHAR(20) NOT NULL UNIQUE,
    app_id VARCHAR(100),
    app_secret VARCHAR(200),
    redirect_uri TEXT,
    is_enabled BOOLEAN DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE token_blacklist (
    id TEXT PRIMARY KEY,
    token_hash TEXT NOT NULL,      -- token 的哈希值
    user_id TEXT,                 -- 用户 ID
    expires_at TEXT NOT NULL,     -- token 原始过期时间
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    reason TEXT DEFAULT 'logout'  -- 登出原因
);
CREATE INDEX idx_token_blacklist_token_hash ON token_blacklist(token_hash);
CREATE INDEX idx_token_blacklist_expires ON token_blacklist(expires_at);
CREATE INDEX idx_jobs_target_device_id ON jobs(target_device_id);
CREATE INDEX idx_sms_codes_phone_expires ON sms_codes(phone, expires_at);
CREATE UNIQUE INDEX idx_users_phone_number ON users(phone_number);
CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    tenant_id TEXT NOT NULL,
    agent_id TEXT,
    agent_config TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL, heartbeat_trust_config TEXT NOT NULL DEFAULT '', heartbeat_config TEXT NOT NULL DEFAULT '', require_action_confirm BOOLEAN DEFAULT 1,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);
CREATE INDEX idx_workspaces_tenant ON workspaces(tenant_id);
CREATE INDEX idx_workspaces_agent ON workspaces(agent_id);
CREATE TABLE batch_commands (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    command_name TEXT NOT NULL,
    command_type TEXT NOT NULL DEFAULT 'custom',
    parameters TEXT, -- JSON string
    total_devices INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending',
    -- status: pending, running, completed, partial_failure, failed
    submitted_by TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at DATETIME,
    UNIQUE(workspace_id, idempotency_key)
);
CREATE INDEX idx_batch_commands_workspace_id ON batch_commands(workspace_id);
CREATE INDEX idx_batch_commands_idempotency ON batch_commands(workspace_id, idempotency_key);
CREATE INDEX idx_batch_commands_status ON batch_commands(status);
CREATE INDEX idx_batch_commands_created_at ON batch_commands(created_at);
CREATE TABLE batch_command_items (
    id TEXT PRIMARY KEY,
    batch_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    device_name TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    -- status: pending, sent, success, failure, timeout
    result_message TEXT,
    command_id TEXT,
    executed_at DATETIME,
    completed_at DATETIME,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (batch_id) REFERENCES batch_commands(id) ON DELETE CASCADE
);
CREATE INDEX idx_batch_command_items_batch_id ON batch_command_items(batch_id);
CREATE INDEX idx_batch_command_items_device_id ON batch_command_items(device_id);
CREATE INDEX idx_batch_command_items_status ON batch_command_items(status);
CREATE TABLE chat_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_key TEXT NOT NULL UNIQUE,
    agent_id TEXT NOT NULL,
    user_id TEXT,
    workspace_id TEXT,
    label TEXT,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_chat_sessions_agent ON chat_sessions(agent_id);
CREATE INDEX idx_chat_sessions_user ON chat_sessions(user_id);
CREATE INDEX idx_chat_sessions_workspace ON chat_sessions(workspace_id);
CREATE TABLE chat_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_key TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    run_id TEXT,
    tool_call_id TEXT,
    tool_name TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (session_key) REFERENCES chat_sessions(session_key)
);
CREATE INDEX idx_chat_messages_session ON chat_messages(session_key);
CREATE INDEX idx_chat_messages_run ON chat_messages(run_id);
CREATE TABLE chat_compacted_sessions (
    session_key TEXT PRIMARY KEY,
    system_messages TEXT NOT NULL,
    summary_message TEXT,
    recent_messages TEXT NOT NULL,
    compacted_at INTEGER NOT NULL,
    original_message_count INTEGER NOT NULL
);
CREATE TABLE agent_configs (
    agent_id TEXT PRIMARY KEY,
    config TEXT NOT NULL,
    config_hash TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE agent_tools (
    agent_id TEXT PRIMARY KEY,
    tool_overrides TEXT NOT NULL DEFAULT ('{"enabled": [], "disabled": []}'),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE "api_keys" (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    prefix TEXT NOT NULL,
    permissions TEXT NOT NULL DEFAULT '["read"]',
    rate_limit INTEGER NOT NULL DEFAULT 60,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    is_revoked INTEGER NOT NULL DEFAULT 0,
    last_used_at TEXT,
    last_used_ip TEXT,
    request_count INTEGER NOT NULL DEFAULT 0,
    expires_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);
CREATE INDEX idx_api_keys_workspace ON api_keys(workspace_id);
CREATE INDEX idx_api_keys_prefix ON api_keys(prefix);
CREATE TABLE device_memory (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    agent_id TEXT NOT NULL DEFAULT 'default',
    device_id TEXT NOT NULL,
    snapshot_data TEXT NOT NULL,  -- JSON 格式的设备状态快照
    snapshot_time INTEGER NOT NULL,  -- Unix timestamp milliseconds
    created_at TEXT DEFAULT (datetime('now')),
    UNIQUE(workspace_id, agent_id, device_id)
);
CREATE INDEX idx_device_memory_lookup
ON device_memory(workspace_id, agent_id, device_id, snapshot_time DESC);
CREATE TRIGGER keep_device_memory_limit
AFTER INSERT ON device_memory
BEGIN
    DELETE FROM device_memory
    WHERE workspace_id = NEW.workspace_id
      AND agent_id = NEW.agent_id
      AND device_id = NEW.device_id
      AND id NOT IN (
          SELECT id FROM device_memory
          WHERE workspace_id = NEW.workspace_id
            AND agent_id = NEW.agent_id
            AND device_id = NEW.device_id
          ORDER BY snapshot_time DESC
          LIMIT 100
      );
END;
CREATE TABLE agents (
    agent_id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_agents_workspace ON agents(workspace_id);
CREATE TABLE cron_runs (
    id            TEXT PRIMARY KEY,
    job_id        TEXT NOT NULL,
    workspace_id  TEXT NOT NULL,
    started_at    TEXT NOT NULL,
    ended_at      TEXT,
    duration_ms   INTEGER,
    status        TEXT NOT NULL
                      CHECK (status IN ('pending', 'running', 'success', 'failed')),
    output        TEXT,
    error_message TEXT,
    trigger_type  TEXT NOT NULL DEFAULT 'schedule',
    triggered_by  TEXT,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (job_id) REFERENCES cron_jobs(id) ON DELETE CASCADE
);
CREATE INDEX idx_cron_runs_job_id
    ON cron_runs(job_id);
CREATE INDEX idx_cron_runs_status
    ON cron_runs(status);
CREATE INDEX idx_cron_runs_started
    ON cron_runs(started_at);
CREATE INDEX idx_cron_runs_workspace
    ON cron_runs(workspace_id);
CREATE INDEX idx_events_device_id ON events(device_id);
CREATE INDEX idx_events_event_level ON events(event_level);
CREATE INDEX idx_events_event_type ON events(event_type);
CREATE INDEX idx_notification_channels_workspace ON notification_channels(workspace_id);
CREATE INDEX idx_notification_rules_workspace ON notification_rules(workspace_id);
CREATE INDEX idx_notification_history_workspace ON notification_history(workspace_id);
CREATE UNIQUE INDEX idx_users_phone_unique ON users(phone) WHERE phone IS NOT NULL;
CREATE INDEX idx_roles_workspace ON roles(workspace_id);
CREATE TABLE system_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_system_settings_key ON system_settings(key);
CREATE TABLE driver_installations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    driver_name TEXT NOT NULL,
    version TEXT NOT NULL,
    file_path TEXT NOT NULL,
    checksum TEXT NOT NULL,
    protocol_type TEXT,
    installed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(workspace_id, driver_name, version)
);
CREATE INDEX idx_driver_installations_workspace ON driver_installations(workspace_id);
CREATE INDEX idx_driver_installations_driver ON driver_installations(driver_name);
CREATE TABLE workspace_driver_preferences (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    driver_name TEXT NOT NULL,
    preferred_version TEXT NOT NULL,
    auto_update INTEGER DEFAULT 0,
    UNIQUE(workspace_id, driver_name)
);
CREATE INDEX idx_workspace_driver_prefs_workspace ON workspace_driver_preferences(workspace_id);
CREATE TABLE agent_memories (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    zone TEXT NOT NULL DEFAULT 'general',
    content TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'user',
    confidence TEXT NOT NULL DEFAULT 'medium',
    tags TEXT NOT NULL DEFAULT '[]',
    pinned INTEGER NOT NULL DEFAULT 0,
    supersedes TEXT,
    device_id TEXT,
    snapshot_data TEXT,
    snapshot_time INTEGER,
    effectiveness REAL NOT NULL DEFAULT 1.0,
    load_count INTEGER NOT NULL DEFAULT 0,
    reference_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_memories_ws_agent ON agent_memories(workspace_id, agent_id);
CREATE INDEX idx_memories_zone ON agent_memories(workspace_id, agent_id, zone);
CREATE INDEX idx_memories_pinned ON agent_memories(workspace_id, agent_id, pinned);
CREATE INDEX idx_memories_effectiveness ON agent_memories(workspace_id, agent_id, effectiveness DESC);
CREATE INDEX idx_memories_source ON agent_memories(workspace_id, agent_id, source);
CREATE TABLE reflection_queue (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    session_key TEXT NOT NULL,
    candidate_type TEXT NOT NULL,
    candidate_data TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    reviewed_at TEXT,
    reviewer_note TEXT
);
CREATE INDEX idx_reflection_queue_status
    ON reflection_queue(workspace_id, agent_id, status);
CREATE TABLE reflection_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT,
    label TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_reflection_log_session
    ON reflection_log(session_id, created_at DESC);
CREATE TABLE knowledge_entities (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    source_document_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    properties TEXT NOT NULL DEFAULT '{}',
    tags TEXT NOT NULL DEFAULT '[]',
    file_ids TEXT NOT NULL DEFAULT '[]',
    device_id TEXT,
    confidence REAL NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE knowledge_relations (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    source_entity_id TEXT NOT NULL REFERENCES knowledge_entities(id) ON DELETE CASCADE,
    target_entity_id TEXT NOT NULL REFERENCES knowledge_entities(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL,
    properties TEXT NOT NULL DEFAULT '{}',
    confidence REAL NOT NULL DEFAULT 0
);
CREATE TABLE knowledge_parse_jobs (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    error_message TEXT,
    result_summary TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_knowledge_entities_workspace ON knowledge_entities(workspace_id);
CREATE INDEX idx_knowledge_entities_tags ON knowledge_entities(tags);
CREATE INDEX idx_knowledge_entities_device ON knowledge_entities(device_id);
CREATE INDEX idx_knowledge_relations_workspace ON knowledge_relations(workspace_id);
CREATE TABLE agent_actions (
    id              TEXT PRIMARY KEY,
    workspace_id    TEXT NOT NULL,
    agent_id        TEXT NOT NULL,
    alarm_id        TEXT,
    device_id       TEXT,
    event_type      TEXT NOT NULL,
    action_type     TEXT NOT NULL,
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_agent_actions_workspace ON agent_actions(workspace_id);
CREATE INDEX idx_agent_actions_alarm ON agent_actions(alarm_id);
CREATE INDEX idx_agent_actions_agent ON agent_actions(agent_id);
CREATE INDEX idx_agent_actions_created ON agent_actions(created_at);
CREATE INDEX idx_agent_actions_ws_event_created
    ON agent_actions(workspace_id, event_type, created_at);
CREATE TABLE heartbeat_tasks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    priority TEXT NOT NULL DEFAULT 'low',
    text TEXT NOT NULL,
    paused INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(workspace_id, id)
);
CREATE INDEX idx_heartbeat_tasks_workspace
    ON heartbeat_tasks(workspace_id);
CREATE TABLE agent_dead_letters (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    failure_reason TEXT NOT NULL,
    enqueued_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_agent_dlq_workspace ON agent_dead_letters(workspace_id, enqueued_at);
CREATE TABLE "thing_templates" (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    display_name TEXT NOT NULL,
    description TEXT,
    version TEXT NOT NULL,
    author TEXT,
    category TEXT NOT NULL,
    manufacturer TEXT,
    device_type TEXT NOT NULL,
    thing_type TEXT NOT NULL DEFAULT 'device',
    protocol_type TEXT,
    driver_name TEXT,
    tags TEXT NOT NULL DEFAULT '[]',
    device_info TEXT NOT NULL DEFAULT '{}',
    properties TEXT NOT NULL DEFAULT '[]',
    actions TEXT NOT NULL DEFAULT '[]',
    events TEXT NOT NULL DEFAULT '[]',
    default_knowledge TEXT,
    is_builtin INTEGER DEFAULT 0,
    is_active INTEGER DEFAULT 1,
    workspace_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (category) REFERENCES template_categories(name)
);
CREATE UNIQUE INDEX idx_thing_templates_name_workspace
    ON thing_templates(COALESCE(workspace_id, ''), name);
CREATE INDEX idx_thing_templates_category ON thing_templates(category);
CREATE INDEX idx_thing_templates_device_type ON thing_templates(device_type);
CREATE INDEX idx_thing_templates_thing_type ON thing_templates(thing_type);
CREATE INDEX idx_thing_templates_manufacturer ON thing_templates(manufacturer);
CREATE INDEX idx_thing_templates_protocol_type ON thing_templates(protocol_type);
CREATE INDEX idx_thing_templates_driver_name ON thing_templates(driver_name);
CREATE INDEX idx_thing_templates_is_builtin ON thing_templates(is_builtin);
CREATE INDEX idx_thing_templates_is_active ON thing_templates(is_active);
CREATE INDEX idx_thing_templates_created_at ON thing_templates(created_at);
CREATE INDEX idx_thing_templates_workspace ON thing_templates(workspace_id);
CREATE TABLE "devices" (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    display_name TEXT,
    thing_type TEXT NOT NULL DEFAULT 'device',
    device_type TEXT,
    address TEXT,
    description TEXT,
    position TEXT,
    driver_name TEXT,
    device_model TEXT,
    protocol_type TEXT,
    factory_name TEXT,
    linked_data TEXT,
    driver_options TEXT,
    state INTEGER NOT NULL DEFAULT 0,
    parent_id TEXT,
    organization_id TEXT,
    tenant_id TEXT,
    workspace_id TEXT,
    linked_gateway TEXT,
    fingerprint TEXT,
    template_id TEXT,
    ontology_summary TEXT,
    summary_status TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (parent_id) REFERENCES devices(id) ON DELETE RESTRICT,
    FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE SET NULL,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE SET NULL,
    FOREIGN KEY (template_id) REFERENCES thing_templates(id) ON DELETE SET NULL
);
CREATE UNIQUE INDEX idx_devices_name_workspace
    ON devices(COALESCE(workspace_id, ''), name);
CREATE INDEX idx_devices_device_type ON devices(device_type);
CREATE INDEX idx_devices_state ON devices(state);
CREATE INDEX idx_devices_parent_id ON devices(parent_id);
CREATE INDEX idx_devices_organization_id ON devices(organization_id);
CREATE INDEX idx_devices_tenant_id ON devices(tenant_id);
CREATE INDEX idx_devices_workspace ON devices(workspace_id);
CREATE INDEX idx_devices_driver_name ON devices(driver_name);
CREATE INDEX idx_devices_factory_name ON devices(factory_name);
CREATE INDEX idx_devices_linked_gateway ON devices(linked_gateway);
CREATE INDEX idx_devices_fingerprint ON devices(fingerprint);
CREATE INDEX idx_devices_thing_type ON devices(thing_type);
CREATE INDEX idx_devices_template_id ON devices(template_id);
CREATE TABLE "tags" (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL CHECK (type IN ('device', 'app', 'thing')),
    name TEXT NOT NULL,
    description TEXT,
    color TEXT,
    tenant_id TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL
);
CREATE UNIQUE INDEX idx_tags_type_name_tenant
    ON tags(COALESCE(tenant_id, ''), type, name);
CREATE INDEX idx_tags_type ON tags(type);
CREATE INDEX idx_tags_name ON tags(name);
CREATE INDEX idx_tags_tenant_id ON tags(tenant_id);
CREATE TABLE "resources" (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    device_id TEXT,
    resource_type TEXT NOT NULL DEFAULT 'document',
    name TEXT NOT NULL,
    file_path TEXT NOT NULL DEFAULT '',
    content TEXT,
    tags TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL, description TEXT, file_size INTEGER, metadata TEXT,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE SET NULL
);
CREATE INDEX idx_resources_workspace ON resources(workspace_id);
CREATE INDEX idx_resources_type ON resources(resource_type);
CREATE INDEX idx_resources_name ON resources(name);
CREATE INDEX idx_resources_workspace_type ON resources(workspace_id, resource_type);
CREATE INDEX idx_resources_device_id ON resources(device_id);
CREATE TABLE "thing_properties" (
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
CREATE INDEX idx_thing_properties_device_id ON thing_properties(device_id);
CREATE TABLE "thing_actions" (
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
CREATE INDEX idx_thing_actions_device_id ON thing_actions(device_id);
CREATE TABLE "device_alarm_rules" (
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
CREATE INDEX idx_device_alarm_rules_device_id ON device_alarm_rules(device_id);
CREATE INDEX idx_device_alarm_rules_is_enabled ON device_alarm_rules(is_enabled);
CREATE TABLE "device_alarms" (
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
CREATE INDEX idx_device_alarms_device_id ON device_alarms(device_id);
CREATE INDEX idx_device_alarms_alarm_level ON device_alarms(alarm_level);
CREATE INDEX idx_device_alarms_alarm_time ON device_alarms(alarm_time);
CREATE INDEX idx_device_alarms_is_acknowledged ON device_alarms(is_acknowledged);
CREATE INDEX idx_device_alarms_is_resolved ON device_alarms(is_resolved);
CREATE UNIQUE INDEX idx_events_status_dedup
    ON events(event_type, event_subtype, device_id)
    WHERE is_status = 1 AND device_id IS NOT NULL;
CREATE TABLE "cron_jobs" (
    id              TEXT PRIMARY KEY,
    workspace_id    TEXT NOT NULL,
    name            TEXT NOT NULL,
    description     TEXT,
    job_type        TEXT NOT NULL DEFAULT 'shell'
                    CHECK (job_type IN ('shell', 'agent', 'device_command', 'event_retention')),
    cron_expression TEXT NOT NULL,
    config          TEXT NOT NULL DEFAULT '{}',
    timeout_seconds INTEGER DEFAULT 300,
    max_retries     INTEGER DEFAULT 3,
    is_enabled      BOOLEAN NOT NULL DEFAULT true,
    is_running      BOOLEAN NOT NULL DEFAULT false,
    last_run_at     TEXT,
    last_run_status TEXT,
    last_run_error  TEXT,
    next_run_at     TEXT,
    run_count       INTEGER DEFAULT 0,
    success_count   INTEGER DEFAULT 0,
    fail_count      INTEGER DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    created_by      TEXT,
    UNIQUE(workspace_id, name)
);
CREATE INDEX idx_cron_jobs_workspace ON cron_jobs(workspace_id);
CREATE INDEX idx_cron_jobs_due ON cron_jobs(is_enabled, is_running, next_run_at);
CREATE INDEX idx_events_workspace_created
    ON events(workspace_id, created_at DESC);
CREATE TABLE workspace_autonomy_policy (
    workspace_id TEXT PRIMARY KEY,
    mode TEXT NOT NULL DEFAULT 'off' CHECK (mode IN ('off','diagnose','act')),
    allowed_actions TEXT NOT NULL DEFAULT '["*"]',
    denied_actions TEXT NOT NULL DEFAULT '[]',
    max_actions_per_run INTEGER NOT NULL DEFAULT 3,
    max_actions_per_hour INTEGER NOT NULL DEFAULT 30,
    constraints TEXT,
    updated_by TEXT,
    updated_at TEXT
);
CREATE TABLE policy_rules (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    category TEXT NOT NULL,
    action TEXT NOT NULL,
    target TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    reason TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_policy_rules_ws ON policy_rules(workspace_id, category);
CREATE TABLE agent_runs (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    trigger_type TEXT NOT NULL,
    trigger_context TEXT,
    outcome TEXT NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    report TEXT NOT NULL DEFAULT '{}',
    verified INTEGER NOT NULL DEFAULT 0,
    tool_calls INTEGER NOT NULL DEFAULT 0,
    tokens INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    problem_key TEXT,
    dedup_key TEXT,
    acked_at TEXT,
    acked_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
, status TEXT NOT NULL DEFAULT 'completed');
CREATE INDEX idx_agent_runs_ws_created ON agent_runs(workspace_id, created_at);
CREATE INDEX idx_agent_runs_problem ON agent_runs(workspace_id, problem_key, created_at);
CREATE INDEX idx_agent_runs_dedup ON agent_runs(workspace_id, dedup_key, created_at);
CREATE VIEW agent_daily_cost AS
SELECT workspace_id, date(created_at) AS day,
       COUNT(*) AS runs, SUM(tokens) AS tokens, SUM(duration_ms) AS duration_ms
FROM agent_runs GROUP BY workspace_id, date(created_at);
