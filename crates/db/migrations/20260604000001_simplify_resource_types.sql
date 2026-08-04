-- Simplify resource_type from 5+ values to 2: 'file' and 'document'
-- 'scene', 'device_model', 'image', 'glb' all become 'file'
-- 'document' stays as-is

UPDATE resources SET resource_type = 'file'
 WHERE resource_type IN ('scene', 'device_model', 'image', 'glb');
