-- Token 黑名单表
-- 用于存储已登出/失效的 token

CREATE TABLE IF NOT EXISTS token_blacklist (
    id TEXT PRIMARY KEY,
    token_hash TEXT NOT NULL,      -- token 的哈希值
    user_id TEXT,                 -- 用户 ID
    expires_at TEXT NOT NULL,     -- token 原始过期时间
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    reason TEXT DEFAULT 'logout'  -- 登出原因
);

CREATE INDEX idx_token_blacklist_token_hash ON token_blacklist(token_hash);
CREATE INDEX idx_token_blacklist_expires ON token_blacklist(expires_at);
