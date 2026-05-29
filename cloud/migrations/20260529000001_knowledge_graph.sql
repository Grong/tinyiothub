-- Knowledge documents (source of truth, Markdown)
CREATE TABLE knowledge_documents (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    parse_status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Knowledge entities (AI-extracted nodes)
CREATE TABLE knowledge_entities (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    source_document_id TEXT NOT NULL REFERENCES knowledge_documents(id) ON DELETE CASCADE,
    entity_type TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    properties TEXT NOT NULL DEFAULT '{}',
    tags TEXT NOT NULL DEFAULT '[]',
    file_ids TEXT NOT NULL DEFAULT '[]',
    device_id TEXT,
    confidence REAL NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_knowledge_entities_workspace ON knowledge_entities(workspace_id);
CREATE INDEX idx_knowledge_entities_tags ON knowledge_entities(tags);
CREATE INDEX idx_knowledge_entities_device ON knowledge_entities(device_id);

-- Knowledge relations (AI-extracted edges)
CREATE TABLE knowledge_relations (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    source_entity_id TEXT NOT NULL REFERENCES knowledge_entities(id) ON DELETE CASCADE,
    target_entity_id TEXT NOT NULL REFERENCES knowledge_entities(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL,
    properties TEXT NOT NULL DEFAULT '{}',
    confidence REAL NOT NULL DEFAULT 0
);

CREATE INDEX idx_knowledge_relations_workspace ON knowledge_relations(workspace_id);

-- Async parse job tracking
CREATE TABLE knowledge_parse_jobs (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL REFERENCES knowledge_documents(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending',
    error_message TEXT,
    result_summary TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
