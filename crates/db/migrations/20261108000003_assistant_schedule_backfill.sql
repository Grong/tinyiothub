-- One-time backfill of pre-existing assistant_schedules rows from the legacy
-- autonomy policy. New rows are mapped by `ensure`; rows created before this
-- migration would otherwise keep the column default (`ask`, enabled) and
-- silently resume wakes for a legacy `off` workspace. Kept as a separate
-- migration so it runs exactly once (and still covers DBs that applied
-- 20261108000002 before it carried the backfill).
UPDATE assistant_schedules SET enabled = 0
  WHERE workspace_id IN (SELECT workspace_id FROM workspace_autonomy_policy WHERE mode = 'off');

UPDATE assistant_schedules SET permission_mode = 'read_only'
  WHERE workspace_id IN (SELECT workspace_id FROM workspace_autonomy_policy WHERE mode = 'diagnose');
