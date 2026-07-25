-- Add missing columns to resources table (workspace module WorkspaceResource struct)
ALTER TABLE resources ADD COLUMN description TEXT;
ALTER TABLE resources ADD COLUMN file_size INTEGER;
ALTER TABLE resources ADD COLUMN metadata TEXT;
ALTER TABLE resources ADD COLUMN parse_status TEXT;
