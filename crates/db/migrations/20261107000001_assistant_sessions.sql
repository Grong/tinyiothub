ALTER TABLE chat_sessions ADD COLUMN kind TEXT NOT NULL DEFAULT 'chat';
ALTER TABLE chat_sessions ADD COLUMN status TEXT NOT NULL DEFAULT 'active';
CREATE INDEX IF NOT EXISTS idx_chat_sessions_workspace_updated
    ON chat_sessions(workspace_id, updated_at);
