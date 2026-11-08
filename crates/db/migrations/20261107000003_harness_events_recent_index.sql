CREATE INDEX IF NOT EXISTS idx_session_events_occurred_at
    ON session_events(occurred_at);
