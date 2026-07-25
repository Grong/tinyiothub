-- Backfill resource tags based on file extension for all existing resources

UPDATE resources SET tags = '["PDF"]'
WHERE (file_path LIKE '%.pdf' OR name LIKE '%.pdf')
  AND (tags IS NULL OR tags = '[]' OR tags = '');

UPDATE resources SET tags = '["图片"]'
WHERE (file_path LIKE '%.png' OR file_path LIKE '%.jpg' OR file_path LIKE '%.jpeg'
   OR file_path LIKE '%.gif' OR file_path LIKE '%.svg' OR file_path LIKE '%.webp'
   OR name LIKE '%.png' OR name LIKE '%.jpg' OR name LIKE '%.jpeg'
   OR name LIKE '%.gif' OR name LIKE '%.svg' OR name LIKE '%.webp')
  AND (tags IS NULL OR tags = '[]' OR tags = '');

UPDATE resources SET tags = '["3D模型"]'
WHERE (file_path LIKE '%.glb' OR file_path LIKE '%.gltf' OR file_path LIKE '%.obj' OR file_path LIKE '%.stl'
   OR name LIKE '%.glb' OR name LIKE '%.gltf' OR name LIKE '%.obj' OR name LIKE '%.stl')
  AND (tags IS NULL OR tags = '[]' OR tags = '');

UPDATE resources SET tags = '["文档"]'
WHERE (file_path LIKE '%.doc' OR file_path LIKE '%.docx' OR file_path LIKE '%.md' OR file_path LIKE '%.txt'
   OR name LIKE '%.doc' OR name LIKE '%.docx' OR name LIKE '%.md' OR name LIKE '%.txt')
  AND (tags IS NULL OR tags = '[]' OR tags = '');

UPDATE resources SET tags = '["表格"]'
WHERE (file_path LIKE '%.xls' OR file_path LIKE '%.xlsx' OR file_path LIKE '%.csv'
   OR name LIKE '%.xls' OR name LIKE '%.xlsx' OR name LIKE '%.csv')
  AND (tags IS NULL OR tags = '[]' OR tags = '');

UPDATE resources SET tags = '["文件"]'
WHERE tags IS NULL OR tags = '[]' OR tags = '';
