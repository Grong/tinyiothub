ALTER TABLE session_events ADD COLUMN turn_id TEXT;
ALTER TABLE session_events ADD COLUMN step_id TEXT;
ALTER TABLE session_events ADD COLUMN attempt_id TEXT;

UPDATE session_events
SET turn_id = json_extract(payload_json, '$.turnId')
WHERE turn_id IS NULL
  AND json_extract(payload_json, '$.turnId') IS NOT NULL;

UPDATE session_events
SET turn_id = run_id
WHERE turn_id IS NULL
  AND type = 'turn.started'
  AND run_id IS NOT NULL;

UPDATE session_events
SET step_id = json_extract(payload_json, '$.stepId')
WHERE step_id IS NULL
  AND json_extract(payload_json, '$.stepId') IS NOT NULL;

UPDATE session_events
SET attempt_id = json_extract(payload_json, '$.attemptId')
WHERE attempt_id IS NULL
  AND json_extract(payload_json, '$.attemptId') IS NOT NULL;

UPDATE session_events AS event
SET turn_id = (
    SELECT COALESCE(json_extract(started.payload_json, '$.turnId'), started.run_id)
    FROM session_events AS started
    WHERE started.execution_mode = event.execution_mode
      AND started.session_id = event.session_id
      AND started.type = 'turn.started'
      AND started.seq <= event.seq
    ORDER BY started.seq DESC
    LIMIT 1
)
WHERE event.turn_id IS NULL
  AND EXISTS (
    SELECT 1
    FROM session_events AS started
    WHERE started.execution_mode = event.execution_mode
      AND started.session_id = event.session_id
      AND started.type = 'turn.started'
      AND started.seq <= event.seq
  );

CREATE INDEX idx_session_events_mode_session_turn_seq
    ON session_events (execution_mode, session_id, turn_id, seq);
