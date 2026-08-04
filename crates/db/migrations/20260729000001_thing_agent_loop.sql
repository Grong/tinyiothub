-- Thing Agent Loop: autonomy policy, unified policy rules, run records
CREATE TABLE workspace_autonomy_policy (
    workspace_id TEXT PRIMARY KEY,
    mode TEXT NOT NULL DEFAULT 'off' CHECK (mode IN ('off','diagnose','act')),
    allowed_actions TEXT NOT NULL DEFAULT '["*"]',
    denied_actions TEXT NOT NULL DEFAULT '[]',
    max_actions_per_run INTEGER NOT NULL DEFAULT 3,
    max_actions_per_hour INTEGER NOT NULL DEFAULT 30,
    constraints TEXT,
    updated_by TEXT,
    updated_at TEXT
);

CREATE TABLE policy_rules (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    category TEXT NOT NULL,
    action TEXT NOT NULL,
    target TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    reason TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_policy_rules_ws ON policy_rules(workspace_id, category);

CREATE TABLE agent_runs (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    trigger_type TEXT NOT NULL,
    trigger_context TEXT,
    outcome TEXT NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    report TEXT NOT NULL DEFAULT '{}',
    verified INTEGER NOT NULL DEFAULT 0,
    tool_calls INTEGER NOT NULL DEFAULT 0,
    tokens INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    problem_key TEXT,
    dedup_key TEXT,
    acked_at TEXT,
    acked_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_agent_runs_ws_created ON agent_runs(workspace_id, created_at);
CREATE INDEX idx_agent_runs_problem ON agent_runs(workspace_id, problem_key, created_at);
CREATE INDEX idx_agent_runs_dedup ON agent_runs(workspace_id, dedup_key, created_at);

CREATE VIEW agent_daily_cost AS
SELECT workspace_id, date(created_at) AS day,
       COUNT(*) AS runs, SUM(tokens) AS tokens, SUM(duration_ms) AS duration_ms
FROM agent_runs GROUP BY workspace_id, date(created_at);

ALTER TABLE events ADD COLUMN actor TEXT NOT NULL DEFAULT 'device';
