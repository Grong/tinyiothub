PRAGMA foreign_keys = OFF;

-- Fix dangling foreign keys and add workspace cascade for knowledge graph tables.
--
-- Background:
-- 1. knowledge_graph.sql created FKs referencing knowledge_documents(id)
-- 2. unify_resources.sql later DROPped knowledge_documents
-- 3. knowledge_entities.source_document_id and knowledge_parse_jobs.document_id
--    both had dangling FK references to a table that no longer exists
-- 4. knowledge_entities.workspace_id and knowledge_relations.workspace_id
--    had no FK at all, so workspace deletion left orphan rows
-- 5. PRAGMA foreign_keys was never enabled (fixed in pool.rs), so these were
--    silently tolerated
--
-- After unify_resources.sql drops knowledge_documents, any statement referencing
-- knowledge_entities or knowledge_parse_jobs fails with "no such table:
-- main.knowledge_documents" — even with foreign_keys = OFF. We work around this
-- by recreating a shell knowledge_documents table so FK metadata can resolve,
-- fixing the dangling FKs, then dropping the shell.
--
-- This migration:
-- - Removes the dangling FK on knowledge_entities.source_document_id
-- - Removes the dangling FK on knowledge_parse_jobs.document_id
-- - Adds workspace_id FK with ON DELETE CASCADE to knowledge_entities
-- - Adds workspace_id FK with ON DELETE CASCADE to knowledge_relations

-- Step 1: Recreate knowledge_documents as a shell table so FK metadata resolves
CREATE TABLE knowledge_documents (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    parse_status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Step 2: Clean up orphaned rows before adding FK constraints
DELETE FROM knowledge_relations WHERE workspace_id NOT IN (SELECT id FROM workspaces);
DELETE FROM knowledge_entities WHERE workspace_id NOT IN (SELECT id FROM workspaces);
DELETE FROM knowledge_parse_jobs WHERE document_id NOT IN (SELECT id FROM resources);

-- Step 3: Recreate knowledge_entities — remove FK → knowledge_documents, add workspace CASCADE
ALTER TABLE knowledge_entities RENAME TO knowledge_entities_old;

CREATE TABLE knowledge_entities (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    source_document_id TEXT NOT NULL,
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

INSERT INTO knowledge_entities SELECT * FROM knowledge_entities_old;
DROP TABLE knowledge_entities_old;

-- Step 4: Recreate knowledge_relations — add workspace CASCADE
ALTER TABLE knowledge_relations RENAME TO knowledge_relations_old;

CREATE TABLE knowledge_relations (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    source_entity_id TEXT NOT NULL REFERENCES knowledge_entities(id) ON DELETE CASCADE,
    target_entity_id TEXT NOT NULL REFERENCES knowledge_entities(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL,
    properties TEXT NOT NULL DEFAULT '{}',
    confidence REAL NOT NULL DEFAULT 0
);

INSERT INTO knowledge_relations SELECT * FROM knowledge_relations_old;
DROP TABLE knowledge_relations_old;

-- Step 5: Recreate knowledge_parse_jobs — remove FK → knowledge_documents
ALTER TABLE knowledge_parse_jobs RENAME TO knowledge_parse_jobs_old;

CREATE TABLE knowledge_parse_jobs (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    error_message TEXT,
    result_summary TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO knowledge_parse_jobs SELECT * FROM knowledge_parse_jobs_old;
DROP TABLE knowledge_parse_jobs_old;

-- Step 6: Drop the shell knowledge_documents table
DROP TABLE knowledge_documents;

-- Step 7: Recreate indexes
CREATE INDEX IF NOT EXISTS idx_knowledge_entities_workspace ON knowledge_entities(workspace_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_entities_tags ON knowledge_entities(tags);
CREATE INDEX IF NOT EXISTS idx_knowledge_entities_device ON knowledge_entities(device_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_relations_workspace ON knowledge_relations(workspace_id);
