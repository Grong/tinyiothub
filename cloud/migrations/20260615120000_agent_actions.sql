CREATE TABLE IF NOT EXISTS agent_actions (
    id              TEXT PRIMARY KEY,
    workspace_id    TEXT NOT NULL,
    agent_id        TEXT NOT NULL,
    alarm_id        TEXT,
    device_id       TEXT,
    event_type      TEXT NOT NULL,
    action_type     TEXT NOT NULL,
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_agent_actions_workspace ON agent_actions(workspace_id);
CREATE INDEX IF NOT EXISTS idx_agent_actions_alarm ON agent_actions(alarm_id);
CREATE INDEX IF NOT EXISTS idx_agent_actions_agent ON agent_actions(agent_id);
CREATE INDEX IF NOT EXISTS idx_agent_actions_created ON agent_actions(created_at);
