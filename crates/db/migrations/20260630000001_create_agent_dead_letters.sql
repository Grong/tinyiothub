-- Generic dead-letter queue for AI events that exhausted all retries.
-- Used by AiEventHandler retry_with_backoff when persist fails after 5 attempts.
-- Operator inspects/discards via admin API; cleaned after 90 days by retention cron.
CREATE TABLE IF NOT EXISTS agent_dead_letters (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    failure_reason TEXT NOT NULL,
    enqueued_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_agent_dlq_workspace ON agent_dead_letters(workspace_id, enqueued_at);
