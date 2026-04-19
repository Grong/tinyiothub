-- 设备状态快照表
CREATE TABLE IF NOT EXISTS device_memory (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    agent_id TEXT NOT NULL DEFAULT 'default',
    device_id TEXT NOT NULL,
    snapshot_data TEXT NOT NULL,  -- JSON 格式的设备状态快照
    snapshot_time INTEGER NOT NULL,  -- Unix timestamp milliseconds
    created_at TEXT DEFAULT (datetime('now')),
    UNIQUE(workspace_id, agent_id, device_id)
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_device_memory_lookup
ON device_memory(workspace_id, agent_id, device_id, snapshot_time DESC);

-- 保留最近 100 条快照的触发器
CREATE TRIGGER IF NOT EXISTS keep_device_memory_limit
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
