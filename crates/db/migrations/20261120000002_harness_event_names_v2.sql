-- Core lifecycle and tool events use the DSH slash vocabulary.
UPDATE session_events SET type = 'turn/start' WHERE type = 'turn.started';
UPDATE session_events SET type = 'turn/end' WHERE type = 'turn.completed';
UPDATE session_events SET type = 'tool/call' WHERE type = 'tool.called';
UPDATE session_events SET type = 'tool/result' WHERE type = 'tool.returned';

DROP INDEX IF EXISTS idx_session_events_mode_session_turn;
CREATE INDEX idx_session_events_mode_session_turn
    ON session_events (execution_mode, session_id, turn_id, seq DESC)
    WHERE type = 'turn/start' AND turn_id IS NOT NULL;
