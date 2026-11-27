-- Persist the full permission mode for workspace autonomy. The legacy `mode`
-- CHECK only has off/diagnose/act; `permission_mode` carries full_access while
-- `mode` keeps the historical compatibility value.
ALTER TABLE workspace_autonomy_policy ADD COLUMN permission_mode TEXT;
