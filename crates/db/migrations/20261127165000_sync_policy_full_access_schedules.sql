-- A workspace policy set to full_access is the global permission source.
-- Backfill the assistant schedule so pre-existing rows stop forcing approval.
UPDATE assistant_schedules
SET permission_mode = 'full_access'
WHERE permission_mode <> 'full_access'
  AND workspace_id IN (
    SELECT workspace_id FROM workspace_autonomy_policy WHERE permission_mode = 'full_access'
  );
