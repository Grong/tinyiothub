-- Task 14 fix round 1: dead wakes must carry the reason they died so the
-- Supervisor panel and recovery runbook can show an actionable fact instead of
-- "未记录原因".
--
-- Platform schema uses `dead_reason` (not `failure_reason`) because the
-- workspace Harness migration owns `task_wake_outbox.failure_reason`; some
-- tests apply both migration sets to one database and column names must not
-- collide.
ALTER TABLE task_wake_outbox ADD COLUMN dead_reason TEXT;
