-- 创建设备历史数据表
-- 用于存储设备的时序数据（属性值历史）
-- 保留策略：默认7天

CREATE TABLE IF NOT EXISTS device_data (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    property_name TEXT NOT NULL,
    property_value TEXT NOT NULL,
    property_type TEXT NOT NULL DEFAULT 'string', -- number, string, boolean
    unit TEXT,
    quality TEXT DEFAULT 'good', -- good, bad, uncertain
    timestamp TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    -- 外键约束
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE
);

-- 创建索引以提高查询性能
CREATE INDEX IF NOT EXISTS idx_device_data_device_time ON device_data(device_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_device_data_property ON device_data(device_id, property_name);
CREATE INDEX IF NOT EXISTS idx_device_data_timestamp ON device_data(timestamp);
CREATE INDEX IF NOT EXISTS idx_device_data_created ON device_data(created_at);

-- 设备数据统计表（用于快速统计）
CREATE TABLE IF NOT EXISTS device_data_stats (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    property_name TEXT NOT NULL,
    count INTEGER DEFAULT 0,
    min_value REAL,
    max_value REAL,
    avg_value REAL,
    last_updated TEXT NOT NULL,
    
    UNIQUE(device_id, property_name),
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_device_data_stats_device ON device_data_stats(device_id);

-- 插入一些示例数据（基于现有设备）
INSERT INTO device_data (id, device_id, property_name, property_value, property_type, unit, quality, timestamp) VALUES
('data-001', 'device-001', 'temperature', '25.5', 'number', '℃', 'good', datetime('now', '-1 hour')),
('data-002', 'device-001', 'temperature', '26.0', 'number', '℃', 'good', datetime('now', '-50 minutes')),
('data-003', 'device-001', 'temperature', '25.8', 'number', '℃', 'good', datetime('now', '-40 minutes')),
('data-004', 'device-001', 'humidity', '60', 'number', '%', 'good', datetime('now', '-1 hour')),
('data-005', 'device-001', 'humidity', '58', 'number', '%', 'good', datetime('now', '-30 minutes')),
('data-006', 'device-002', 'status', 'online', 'string', NULL, 'good', datetime('now', '-2 hours')),
('data-007', 'device-002', 'status', 'offline', 'string', NULL, 'good', datetime('now', '-1 hour'));

-- 更新统计数据
INSERT INTO device_data_stats (id, device_id, property_name, count, min_value, max_value, avg_value, last_updated) VALUES
('stats-001', 'device-001', 'temperature', 3, 25.5, 26.0, 25.77, datetime('now')),
('stats-002', 'device-001', 'humidity', 2, 58.0, 60.0, 59.0, datetime('now'));
