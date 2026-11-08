ALTER TABLE assistant_schedules ADD COLUMN permission_mode TEXT NOT NULL DEFAULT 'ask';

-- Backfill pre-existing schedule rows from the legacy autonomy policy. New
-- rows are mapped by `ensure`; rows created before this migration would
-- otherwise keep the column default (`ask`, enabled) and silently resume
-- wakes for a legacy `off` workspace.
UPDATE assistant_schedules SET enabled = 0
  WHERE workspace_id IN (SELECT workspace_id FROM workspace_autonomy_policy WHERE mode = 'off');

UPDATE assistant_schedules SET permission_mode = 'read_only'
  WHERE workspace_id IN (SELECT workspace_id FROM workspace_autonomy_policy WHERE mode = 'diagnose');
