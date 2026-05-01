-- 创建设备追踪记录表
-- 用于记录设备的各种操作和状态变更历史

CREATE TABLE IF NOT EXISTS device_traces (
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

-- 创建索引以提高查询性能
CREATE INDEX IF NOT EXISTS idx_device_traces_device_id ON device_traces(device_id);
CREATE INDEX IF NOT EXISTS idx_device_traces_trace_type ON device_traces(trace_type);
CREATE INDEX IF NOT EXISTS idx_device_traces_level ON device_traces(level);
CREATE INDEX IF NOT EXISTS idx_device_traces_category ON device_traces(category);
CREATE INDEX IF NOT EXISTS idx_device_traces_created_at ON device_traces(created_at);
CREATE INDEX IF NOT EXISTS idx_device_traces_user_id ON device_traces(user_id);
CREATE INDEX IF NOT EXISTS idx_device_traces_source ON device_traces(source);

-- 复合索引用于常见查询
CREATE INDEX IF NOT EXISTS idx_device_traces_device_time ON device_traces(device_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_device_traces_device_type ON device_traces(device_id, trace_type);
CREATE INDEX IF NOT EXISTS idx_device_traces_device_level ON device_traces(device_id, level);

-- 插入一些示例追踪记录
-- Test data removed: referenced device-001/002/003 which don't exist in production