-- Agent Skills 表
CREATE TABLE IF NOT EXISTS agent_skills (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id TEXT NOT NULL,
    agent_id TEXT NOT NULL DEFAULT 'default',
    skill_name TEXT NOT NULL,
    skill_content TEXT NOT NULL,  -- Markdown 格式
    skill_type TEXT NOT NULL DEFAULT 'file',  -- 'file' | 'bundled' | 'mcp'
    paths TEXT,  -- JSON array of glob patterns for conditional triggers
    is_hidden BOOLEAN DEFAULT FALSE,  -- 是否在 UI 隐藏
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    UNIQUE(workspace_id, agent_id, skill_name)
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_agent_skills_lookup
ON agent_skills(workspace_id, agent_id);

-- 创建索引用于路径匹配查询
CREATE INDEX IF NOT EXISTS idx_agent_skills_paths
ON agent_skills(workspace_id, agent_id, paths);
