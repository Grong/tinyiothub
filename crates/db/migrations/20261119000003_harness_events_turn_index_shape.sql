-- Replace the first turn-index shape with one the planner can use for
-- GROUP BY run_id without losing the session filter.
DROP INDEX IF EXISTS idx_session_events_mode_session_turn;
CREATE INDEX IF NOT EXISTS idx_session_events_mode_session_turn
    ON session_events (execution_mode, session_id, run_id, seq DESC)
    WHERE type = 'turn.started' AND run_id IS NOT NULL;
