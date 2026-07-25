-- Rename resources.type → resources.resource_type to match workspace module code

ALTER TABLE resources RENAME COLUMN type TO resource_type;
