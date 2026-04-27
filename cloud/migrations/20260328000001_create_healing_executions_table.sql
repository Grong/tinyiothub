-- 自愈执行历史表
-- 创建时间: 2026-03-28

-- ============================================================================
-- 自愈执行记录表
-- ============================================================================
CREATE TABLE IF NOT EXISTS healing_executions (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    level TEXT NOT NULL,
    action_type TEXT NOT NULL,
    target TEXT,
    result TEXT NOT NULL,
    logs TEXT,
    created_at TEXT DEFAULT (datetime('now'))
);

-- ============================================================================
-- 索引
-- ============================================================================
CREATE INDEX IF NOT EXISTS idx_healing_executions_timestamp ON healing_executions(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_healing_executions_level ON healing_executions(level);
CREATE INDEX IF NOT EXISTS idx_healing_executions_tenant ON healing_executions(tenant_id);
