-- Assistant suppression configuration on the platform scheduler row.
--
-- The durable baseline/fence state itself lives in the per-workspace Harness
-- database (20261114000002_harness_assistant_suppression_workspace.sql) so a
-- skip can commit its state, audit events and pending barriers in one
-- workspace transaction. Configuration stays here because the schedule is a
-- platform table.

ALTER TABLE assistant_schedules ADD COLUMN suppression_mode TEXT NOT NULL DEFAULT 'off';
ALTER TABLE assistant_schedules ADD COLUMN reconciliation_seconds INTEGER NOT NULL DEFAULT 1800;
ALTER TABLE assistant_schedules ADD COLUMN suppression_config_version INTEGER NOT NULL DEFAULT 1;
