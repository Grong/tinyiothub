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
INSERT INTO device_traces (id, device_id, trace_type, level, category, title, message, details, source, created_at) VALUES
('trace-001', 'device-001', 'operation', 'info', 'user', '设备属性更新', '用户更新了设备温度阈值', '{"property": "temperature_threshold", "old_value": "80", "new_value": "85"}', 'api', datetime('now', '-2 hours')),
('trace-002', 'device-001', 'status_change', 'info', 'system', '设备上线', '设备重新连接到系统', '{"connection_type": "ethernet", "ip_address": "192.168.1.100"}', 'system', datetime('now', '-1 hour')),
('trace-003', 'device-002', 'error', 'error', 'device', '通信失败', '设备响应超时', '{"timeout_ms": 5000, "retry_count": 3}', 'device', datetime('now', '-30 minutes')),
('trace-004', 'device-002', 'warning', 'warn', 'performance', '性能告警', 'CPU使用率过高', '{"cpu_usage": 92.5, "threshold": 90.0}', 'system', datetime('now', '-15 minutes')),
('trace-005', 'device-003', 'operation', 'info', 'user', '指令执行', '用户执行了重启指令', '{"command": "restart", "execution_id": "exec-001"}', 'api', datetime('now', '-10 minutes'));