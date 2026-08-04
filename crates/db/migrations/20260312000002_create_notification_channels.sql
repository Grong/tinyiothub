-- 通知渠道配置模块
-- 创建时间: 2026-03-12

PRAGMA foreign_keys = ON;

-- ============================================================================
-- 通知渠道配置表
-- ============================================================================
CREATE TABLE IF NOT EXISTS notification_channels (
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
);

-- 索引
CREATE INDEX IF NOT EXISTS idx_notification_channels_type ON notification_channels(channel_type);
CREATE INDEX IF NOT EXISTS idx_notification_channels_enabled ON notification_channels(is_enabled);

-- ============================================================================
-- 初始化数据
-- ============================================================================

-- 默认渠道配置示例
INSERT INTO notification_channels (id, name, channel_type, config, description) VALUES
    ('channel-sms-default', '系统短信', 'sms', 
     '{"provider": "aliyun", "sign_name": "TinyIoT", "template_id": ""}',
     '系统默认短信渠道'),
    ('channel-email-default', '系统邮件', 'email',
     '{"provider": "smtp", "smtp_host": "", "smtp_port": 465, "from": "TinyIoT <noreply@tinyiot.com>"}',
     '系统默认邮件渠道'),
    ('channel-webhook-default', '钉钉 webhook', 'webhook',
     '{"url": "", "method": "POST", "headers": {"Content-Type": "application/json"}}',
     '系统默认钉钉 webhook 渠道');
