-- Cross-session HUD / brain feed: newest 20 events across all sessions.
-- Without this composite expression index the query scans all production
-- events and sorts by (occurred_at, 'session:' || event_id).
CREATE INDEX IF NOT EXISTS idx_session_events_mode_occurred_event
    ON session_events (execution_mode, occurred_at DESC, ('session:' || event_id) DESC);
