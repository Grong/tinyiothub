-- E2 composite 编排原语（spec 2026-10-21-e2 §6）：声明式多设备动作链。
CREATE TABLE agent_composites (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    definition TEXT NOT NULL,   -- JSON（CompositeDef，v=1）
    created_at TEXT NOT NULL
);
CREATE INDEX idx_agent_composites_ws ON agent_composites(workspace_id);
