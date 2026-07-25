-- Backfill: copy template properties and actions into per-thing tables
-- Run after 20260723000001_thing_ontology_rebuild.sql, before 00002 rename
--
-- Two passes:
--   1. Devices WITH template_id: match directly via FK
--   2. Devices WITHOUT template_id: match by device_type to template name

-- First, update template_id for devices without one by matching device_type
-- to the built-in template name (case-insensitive LIKE match)
UPDATE devices SET template_id = (
    SELECT t.id FROM thing_templates t
    WHERE devices.device_type <> ''
      AND (t.name LIKE '%' || devices.device_type || '%'
           OR t.device_type = devices.device_type)
      AND t.workspace_id IS NULL  -- built-in templates only
    LIMIT 1
)
WHERE template_id IS NULL;

-- Properties — for all devices that now have template_id
INSERT INTO thing_properties (id, device_id, name, display_name, data_type, unit, is_read_only, created_at, updated_at)
SELECT
    lower(hex(randomblob(16))),
    d.id,
    json_extract(p.value, '$.name'),
    COALESCE(json_extract(p.value, '$.displayName'), json_extract(p.value, '$.name')),
    COALESCE(json_extract(p.value, '$.dataType'), 'string'),
    json_extract(p.value, '$.unit'),
    COALESCE(json_extract(p.value, '$.isReadOnly'), 0),
    datetime('now'),
    datetime('now')
FROM devices d
JOIN thing_templates t ON t.id = d.template_id
CROSS JOIN json_each(t.properties) AS p
WHERE d.template_id IS NOT NULL
  AND NOT EXISTS (
    SELECT 1 FROM thing_properties dp WHERE dp.device_id = d.id AND dp.name = json_extract(p.value, '$.name')
  );

-- Actions — for all devices that now have template_id
INSERT INTO thing_actions (id, device_id, name, display_name, parameters, created_at)
SELECT
    lower(hex(randomblob(16))),
    d.id,
    json_extract(a.value, '$.name'),
    COALESCE(json_extract(a.value, '$.displayName'), json_extract(a.value, '$.name')),
    json_extract(a.value, '$.parameters'),
    datetime('now')
FROM devices d
JOIN thing_templates t ON t.id = d.template_id
CROSS JOIN json_each(t.actions) AS a
WHERE d.template_id IS NOT NULL
  AND NOT EXISTS (
    SELECT 1 FROM thing_actions dc WHERE dc.device_id = d.id AND dc.name = json_extract(a.value, '$.name')
  );
