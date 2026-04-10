-- Agent configuration: per-agent model, temperature, system prompt settings
CREATE TABLE IF NOT EXISTS agent_configs (
    agent_id TEXT PRIMARY KEY,
    config TEXT NOT NULL,
    config_hash TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Agent tool overrides: per-agent enabled/disabled tool settings
-- stored as JSON: { "enabled": ["tool_id1"], "disabled": ["tool_id2"] }
-- empty arrays mean default (all tools enabled except dangerous ones)
CREATE TABLE IF NOT EXISTS agent_tools (
    agent_id TEXT PRIMARY KEY,
    tool_overrides TEXT NOT NULL DEFAULT ('{"enabled": [], "disabled": []}'),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
