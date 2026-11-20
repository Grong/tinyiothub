-- Session turn pagination: only turn.started rows participate, and a partial
-- index keeps the query from scanning every event in a long session.
CREATE INDEX IF NOT EXISTS idx_session_events_mode_session_turn
    ON session_events (execution_mode, session_id, seq DESC)
    WHERE type = 'turn.started' AND run_id IS NOT NULL;
