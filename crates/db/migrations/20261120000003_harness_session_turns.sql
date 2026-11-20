CREATE TABLE session_turns (
    turn_id        TEXT NOT NULL,
    session_id     TEXT NOT NULL,
    execution_mode TEXT NOT NULL,
    started_seq    INTEGER NOT NULL,
    ended_seq      INTEGER,
    status         TEXT NOT NULL,
    started_at     TEXT NOT NULL,
    ended_at       TEXT,
    origin         TEXT,
    error          TEXT,
    PRIMARY KEY (execution_mode, session_id, turn_id)
);

INSERT INTO session_turns
    (turn_id, session_id, execution_mode, started_seq, status, started_at)
SELECT turn_id, session_id, execution_mode, seq, 'running', occurred_at
FROM session_events
WHERE type = 'turn/start' AND turn_id IS NOT NULL;

UPDATE session_turns
SET status = COALESCE((
        SELECT json_extract(ended.payload_json, '$.outcome')
        FROM session_events AS ended
        WHERE ended.execution_mode = session_turns.execution_mode
          AND ended.session_id = session_turns.session_id
          AND ended.turn_id = session_turns.turn_id
          AND ended.type = 'turn/end'
        ORDER BY ended.seq DESC
        LIMIT 1
    ), 'running'),
    ended_seq = (
        SELECT ended.seq
        FROM session_events AS ended
        WHERE ended.execution_mode = session_turns.execution_mode
          AND ended.session_id = session_turns.session_id
          AND ended.turn_id = session_turns.turn_id
          AND ended.type = 'turn/end'
        ORDER BY ended.seq DESC
        LIMIT 1
    ),
    ended_at = (
        SELECT ended.occurred_at
        FROM session_events AS ended
        WHERE ended.execution_mode = session_turns.execution_mode
          AND ended.session_id = session_turns.session_id
          AND ended.turn_id = session_turns.turn_id
          AND ended.type = 'turn/end'
        ORDER BY ended.seq DESC
        LIMIT 1
    ),
    error = (
        SELECT COALESCE(
            json_extract(ended.payload_json, '$.errorMessage'),
            json_extract(ended.payload_json, '$.error')
        )
        FROM session_events AS ended
        WHERE ended.execution_mode = session_turns.execution_mode
          AND ended.session_id = session_turns.session_id
          AND ended.turn_id = session_turns.turn_id
          AND ended.type = 'turn/end'
        ORDER BY ended.seq DESC
        LIMIT 1
    )
WHERE EXISTS (
    SELECT 1
    FROM session_events AS ended
    WHERE ended.execution_mode = session_turns.execution_mode
      AND ended.session_id = session_turns.session_id
      AND ended.turn_id = session_turns.turn_id
      AND ended.type = 'turn/end'
);

CREATE INDEX idx_session_turns_mode_session_started
    ON session_turns (execution_mode, session_id, started_seq DESC);
