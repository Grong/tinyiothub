-- Backfill: copy template properties and actions into per-thing tables
-- Run after 20260723000001_thing_ontology_rebuild.sql
--
-- For every device that has a template_id, copy the template's
-- properties/actions into device_properties/device_commands.
-- This makes per-thing tables the source of truth, independent of templates.

-- Properties
INSERT INTO device_properties (id, device_id, name, display_name, data_type, unit, is_read_only, created_at, updated_at)
SELECT
    lower(hex(randomblob(16))),  -- random UUID
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
    SELECT 1 FROM device_properties dp WHERE dp.device_id = d.id AND dp.name = json_extract(p.value, '$.name')
  );

-- Actions
INSERT INTO device_commands (id, device_id, name, display_name, parameters, created_at)
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
    SELECT 1 FROM device_commands dc WHERE dc.device_id = d.id AND dc.name = json_extract(a.value, '$.name')
  );
