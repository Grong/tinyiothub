-- 定时任务模块数据库结构
-- 创建时间: 2026-03-12

-- 启用外键约束
PRAGMA foreign_keys = ON;

-- ============================================================================
-- 定时任务表
-- ============================================================================
CREATE TABLE IF NOT EXISTS jobs (
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

-- 任务执行历史表
CREATE TABLE IF NOT EXISTS job_executions (
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

-- 任务依赖关系表
CREATE TABLE IF NOT EXISTS job_dependencies (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    depends_on_job_id TEXT NOT NULL,
    condition TEXT NOT NULL DEFAULT 'success', -- success, failure, always, never
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE,
    FOREIGN KEY (depends_on_job_id) REFERENCES jobs(id) ON DELETE CASCADE,
    UNIQUE(job_id, depends_on_job_id)
);

-- 任务日志表 (详细执行日志)
CREATE TABLE IF NOT EXISTS job_logs (
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

-- ============================================================================
-- 索引
-- ============================================================================
CREATE INDEX IF NOT EXISTS idx_jobs_enabled ON jobs(is_enabled);
CREATE INDEX IF NOT EXISTS idx_jobs_next_run ON jobs(next_run_at);
CREATE INDEX IF NOT EXISTS idx_jobs_type ON jobs(job_type);
CREATE INDEX IF NOT EXISTS idx_job_executions_job_id ON job_executions(job_id);
CREATE INDEX IF NOT EXISTS idx_job_executions_status ON job_executions(status);
CREATE INDEX IF NOT EXISTS idx_job_executions_started ON job_executions(started_at);
CREATE INDEX IF NOT EXISTS idx_job_logs_execution ON job_logs(execution_id);
CREATE INDEX IF NOT EXISTS idx_job_logs_timestamp ON job_logs(timestamp);

-- ============================================================================
-- 初始化数据
-- ============================================================================

-- 插入示例任务
INSERT INTO jobs (id, name, description, job_type, cron_expression, config, is_enabled, tags) VALUES
    ('job-001', '设备状态同步', '每5分钟同步一次设备在线状态', 'http', '*/5 * * * *', 
     '{"url": "/api/devices/sync-status", "method": "POST", "headers": {}}', 
     true, '["系统", "设备"]'),
    ('job-002', '数据清理', '每天凌晨3点清理过期数据', 'script', '0 3 * * *',
     '{"script": "cleanup.sh", "working_dir": "/app/scripts"}',
     true, '["维护", "清理"]'),
    ('job-003', '健康检查', '每分钟检查系统健康状态', 'http', '*/1 * * * *',
     '{"url": "/api/health", "method": "GET"}',
     true, '["系统", "监控"]');
