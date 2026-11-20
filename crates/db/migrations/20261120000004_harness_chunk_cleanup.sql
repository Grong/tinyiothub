-- v2 stores one assistant settlement per step; token-level rows and the old
-- retry marker are deliberately removed at cutover.
DELETE FROM session_events WHERE type IN ('step.text', 'step.thinking', 'turn.retry');
