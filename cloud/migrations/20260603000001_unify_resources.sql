-- Unify workspace_resources and knowledge_documents into a single resources table.
-- Knowledge entities/relations remain as the semantic layer on top.
-- knowledge_parse_jobs table is kept; its document_id now references resources.id.

CREATE TABLE resources (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    resource_type TEXT NOT NULL DEFAULT 'document',
    name TEXT NOT NULL,
    description TEXT,
    content TEXT,
    file_path TEXT NOT NULL DEFAULT '',
    file_size INTEGER,
    tags TEXT NOT NULL DEFAULT '[]',
    metadata TEXT,
    parse_status TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Migrate workspace_resources
INSERT INTO resources (id, workspace_id, resource_type, name, description,
    file_path, tags, metadata, created_at, updated_at)
SELECT id, workspace_id, resource_type, name, description,
    file_path, tags, metadata, created_at, updated_at
FROM workspace_resources;

-- Migrate knowledge_documents (id prefix 'doc-' won't collide with 'res-')
INSERT INTO resources (id, workspace_id, resource_type, name, description,
    content, tags, parse_status, created_at, updated_at)
SELECT id, workspace_id, 'document', title, NULL,
    content, tags, parse_status, created_at, updated_at
FROM knowledge_documents;

-- Drop old tables (frees their index names)
DROP TABLE workspace_resources;
DROP TABLE knowledge_documents;

-- Create indexes after dropping old tables to avoid name conflicts
CREATE INDEX idx_resources_workspace ON resources(workspace_id);
CREATE INDEX idx_resources_type ON resources(resource_type);
CREATE INDEX idx_resources_name ON resources(name);
CREATE INDEX idx_resources_workspace_type ON resources(workspace_id, resource_type);
