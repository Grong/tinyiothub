-- R17: each durable fence owns its attempt identity and start time.
--
-- `latest_attempt_id` is overwritten by `begin_full_attempt`, so recovery must
-- not use it to settle an audit fence that belongs to an older skip attempt.
-- The started-at columns let startup recovery settle only stale fences and
-- never a live Turn.
ALTER TABLE suppression_state ADD COLUMN full_attempt_id TEXT;
ALTER TABLE suppression_state ADD COLUMN full_attempt_started_at TEXT;
ALTER TABLE suppression_state ADD COLUMN audit_attempt_id TEXT;
ALTER TABLE suppression_state ADD COLUMN audit_attempt_started_at TEXT;
