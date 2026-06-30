CREATE INDEX IF NOT EXISTS idx_agent_actions_ws_event_created
    ON agent_actions(workspace_id, event_type, created_at);
