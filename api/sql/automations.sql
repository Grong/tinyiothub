-- 自动化规则表
CREATE TABLE IF NOT EXISTS automations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    trigger_type TEXT NOT NULL DEFAULT 'event',
    event_source_type TEXT,
    event_device_id TEXT,
    event_property TEXT,
    event_condition TEXT,
    cron_expression TEXT,
    conditions TEXT,
    actions TEXT NOT NULL,
    timeout_seconds INTEGER DEFAULT 30,
    retry_count INTEGER DEFAULT 0,
    retry_delay_seconds INTEGER DEFAULT 5,
    cooldown_seconds INTEGER DEFAULT 0,
    priority INTEGER DEFAULT 100,
    enabled INTEGER DEFAULT 1,
    run_count INTEGER DEFAULT 0,
    success_count INTEGER DEFAULT 0,
    fail_count INTEGER DEFAULT 0,
    last_run_at TEXT,
    last_run_status TEXT,
    last_run_error TEXT,
    tags TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    created_by TEXT
);

CREATE INDEX IF NOT EXISTS idx_automations_trigger ON automations(trigger_type, enabled);
CREATE INDEX IF NOT EXISTS idx_automations_priority ON automations(priority);
