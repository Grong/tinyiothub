-- 手机验证码和第三方登录表
-- 创建时间: 2026-03-14

-- 1. 短信验证码表
CREATE TABLE IF NOT EXISTS sms_codes (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    phone VARCHAR(20) NOT NULL,
    code VARCHAR(10) NOT NULL,
    purpose VARCHAR(20) NOT NULL DEFAULT 'login', -- login, register, reset_password
    expires_at TIMESTAMP NOT NULL,
    verified_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    ip_address VARCHAR(45),
    user_agent TEXT
);

CREATE INDEX idx_sms_codes_phone ON sms_codes(phone);
CREATE INDEX idx_sms_codes_code ON sms_codes(code);
CREATE INDEX idx_sms_codes_expires ON sms_codes(expires_at);

-- 2. 第三方登录关联表
CREATE TABLE IF NOT EXISTS social_bindings (
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

-- 3. 微信登录配置表（存储应用配置）
CREATE TABLE IF NOT EXISTS social_configs (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    provider VARCHAR(20) NOT NULL UNIQUE,
    app_id VARCHAR(100),
    app_secret VARCHAR(200),
    redirect_uri TEXT,
    is_enabled BOOLEAN DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- 插入默认的微信配置（需要后续填写）
INSERT INTO social_configs (provider, is_enabled) VALUES ('wechat', 0);
