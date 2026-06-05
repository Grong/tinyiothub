# Workspace Knowledge Graph Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the flat `workspace_resources` file manager with a document-driven knowledge graph system where users write Markdown, AI extracts entities/relations, and the Agent receives structured context injection.

**Architecture:** Three DB tables (knowledge_documents, knowledge_entities, knowledge_relations) + async parse jobs table. LLM-powered parse pipeline via MiniMax/zeroclaw. KnowledgeService independent from WorkspaceService, each with its own repository trait. Frontend: Lit 3 document list view + editor with live preview, parse results panel, and diff view.

**Tech Stack:** Rust/Axum/SQLx/SQLite (backend), Lit 3/TypeScript/Vite (frontend), MiniMax LLM via zeroclaw

---

## File Structure

```
cloud/src/modules/workspace/
  handler/
    knowledge.rs          # NEW — knowledge CRUD + parse + preview + context routes
  types/
    knowledge.rs          # NEW — KnowledgeDocument, KnowledgeEntity, KnowledgeRelation, ParseJob types
  service/
    knowledge.rs          # NEW — KnowledgeService (parse pipeline, context generation, diff, tagging)
  repo/
    knowledge.rs          # NEW — KnowledgeRepository trait + SQLite impl
  handler.rs              # MODIFY — mount knowledge routes under /{id}/knowledge
  mod.rs                  # MODIFY — declare new modules, re-export

cloud/src/modules/agent/tools/
  knowledge.rs            # NEW — search_knowledge tool registration

cloud/src/modules/agent/
  service.rs              # MODIFY — inject knowledge context into system_prompt

web/src/api/
  knowledge.ts            # NEW — knowledge API client (mirrors workspace-resources.ts pattern)

web/src/ui/views/
  knowledge.ts            # NEW — knowledge document list view + editor

web/src/styles/views/
  knowledge.css           # NEW — knowledge view styles
```

---

### Task 1: Database Migration

**Files:**
- Create: `cloud/migrations/20260529_knowledge_graph.sql`

- [ ] **Step 1: Write the migration SQL**

```sql
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
```

- [ ] **Step 2: Apply migration and verify**

Run: `cargo build`
Expected: Migration applied successfully (check for SQLite errors)

- [ ] **Step 3: Commit**

```bash
git add cloud/migrations/20260529_knowledge_graph.sql
git commit -m "feat: add knowledge graph database migration"
```

---

### Task 2: Knowledge Types

**Files:**
- Create: `cloud/src/modules/workspace/types/knowledge.rs`
- Modify: `cloud/src/modules/workspace/types/mod.rs`
- Modify: `cloud/src/modules/workspace/mod.rs`

- [ ] **Step 1: Define knowledge types**

```rust
// cloud/src/modules/workspace/types/knowledge.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocument {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub parse_status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntity {
    pub id: String,
    pub workspace_id: String,
    pub source_document_id: String,
    pub entity_type: String,
    pub name: String,
    pub description: Option<String>,
    pub properties: serde_json::Value,
    pub tags: Vec<String>,
    pub file_ids: Vec<String>,
    pub device_id: Option<String>,
    pub confidence: f32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeRelation {
    pub id: String,
    pub workspace_id: String,
    pub source_entity_id: String,
    pub target_entity_id: String,
    pub relation_type: String,
    pub properties: serde_json::Value,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeParseJob {
    pub id: String,
    pub document_id: String,
    pub status: String,
    pub error_message: Option<String>,
    pub result_summary: Option<ParseResultSummary>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseResultSummary {
    pub entity_count: usize,
    pub relation_count: usize,
    pub diff: Option<ParseDiff>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseDiff {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
}

// Request types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateKnowledgeDocumentRequest {
    pub title: String,
    pub content: String,
    pub tags: Option<Vec<String>>,
    pub file_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateKnowledgeDocumentRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub file_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateKnowledgeEntityRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub entity_type: Option<String>,
    pub properties: Option<serde_json::Value>,
    pub tags: Option<Vec<String>>,
    pub device_id: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewParseRequest {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewParseResponse {
    pub entities: Vec<KnowledgeEntity>,
    pub relations: Vec<KnowledgeRelation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSearchResult {
    pub entity: KnowledgeEntity,
    pub relations: Vec<KnowledgeRelation>,
    pub source_snippet: String,
    pub relevance: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocumentListParams {
    pub q: Option<String>,
    pub tags: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntityListParams {
    pub entity_type: Option<String>,
    pub tags: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSearchParams {
    pub q: String,
    pub entity_type: Option<String>,
    pub tags: Option<String>,
    pub limit: Option<i64>,
}
```

- [ ] **Step 2: Add types module declaration**

Read `cloud/src/modules/workspace/types/mod.rs` first, then add:

```rust
pub mod knowledge;
pub use knowledge::*;
```

- [ ] **Step 3: Update workspace module declaration**

Read `cloud/src/modules/workspace/mod.rs` first. The file currently has:
```rust
pub mod handler;
pub mod repo;
pub mod service;
pub mod types;
pub use handler::create_router;
pub use service::WorkspaceService;
pub use types::*;
```

Add new module declarations:

```rust
pub mod handler;
pub mod repo;
pub mod service;
pub mod types;
pub use handler::create_router;
pub use service::WorkspaceService;
pub use types::*;

pub mod service {
    pub mod knowledge;
    pub use knowledge::KnowledgeService;
}
pub mod repo {
    pub mod knowledge;
    pub use knowledge::KnowledgeRepository;
}
```

- [ ] **Step 4: Build to verify types compile**

Run: `cargo build`
Expected: No compilation errors for the new types

- [ ] **Step 5: Commit**

```bash
git add cloud/src/modules/workspace/types/knowledge.rs cloud/src/modules/workspace/types/mod.rs cloud/src/modules/workspace/mod.rs
git commit -m "feat: add knowledge graph types"
```

---

### Task 3: Knowledge Repository (Backend)

**Files:**
- Create: `cloud/src/modules/workspace/repo/knowledge.rs`
- Modify: `cloud/src/modules/workspace/mod.rs`

- [ ] **Step 1: Read existing repo pattern for reference**

Read `cloud/src/modules/workspace/repo.rs` to see the trait + SQLite FromRow pattern. Key patterns to follow:
- `#[async_trait] pub trait WorkspaceRepository: Send + Sync`
- `FromRow` structs with SQLite mapping
- `QueryBuilder` for dynamic queries
- `Arc<dyn Repository>` DI pattern

- [ ] **Step 2: Write KnowledgeRepository trait and SQLite implementation**

```rust
// cloud/src/modules/workspace/repo/knowledge.rs

use async_trait::async_trait;
use sqlx::SqlitePool;
use crate::shared::id;
use super::super::types::knowledge::*;

// ── Row types for SQLite mapping ──

#[derive(Debug, Clone, sqlx::FromRow)]
struct KnowledgeDocumentRow {
    id: String,
    workspace_id: String,
    title: String,
    content: String,
    tags: String,
    parse_status: String,
    created_at: String,
    updated_at: String,
}

impl From<KnowledgeDocumentRow> for KnowledgeDocument {
    fn from(r: KnowledgeDocumentRow) -> Self {
        Self {
            id: r.id,
            workspace_id: r.workspace_id,
            title: r.title,
            content: r.content,
            tags: serde_json::from_str(&r.tags).unwrap_or_default(),
            parse_status: r.parse_status,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct KnowledgeEntityRow {
    id: String,
    workspace_id: String,
    source_document_id: String,
    entity_type: String,
    name: String,
    description: Option<String>,
    properties: String,
    tags: String,
    file_ids: String,
    device_id: Option<String>,
    confidence: f64,
    created_at: String,
    updated_at: String,
}

impl From<KnowledgeEntityRow> for KnowledgeEntity {
    fn from(r: KnowledgeEntityRow) -> Self {
        Self {
            id: r.id,
            workspace_id: r.workspace_id,
            source_document_id: r.source_document_id,
            entity_type: r.entity_type,
            name: r.name,
            description: r.description,
            properties: serde_json::from_str(&r.properties).unwrap_or_default(),
            tags: serde_json::from_str(&r.tags).unwrap_or_default(),
            file_ids: serde_json::from_str(&r.file_ids).unwrap_or_default(),
            device_id: r.device_id,
            confidence: r.confidence as f32,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct KnowledgeRelationRow {
    id: String,
    workspace_id: String,
    source_entity_id: String,
    target_entity_id: String,
    relation_type: String,
    properties: String,
    confidence: f64,
}

impl From<KnowledgeRelationRow> for KnowledgeRelation {
    fn from(r: KnowledgeRelationRow) -> Self {
        Self {
            id: r.id,
            workspace_id: r.workspace_id,
            source_entity_id: r.source_entity_id,
            target_entity_id: r.target_entity_id,
            relation_type: r.relation_type,
            properties: serde_json::from_str(&r.properties).unwrap_or_default(),
            confidence: r.confidence as f32,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct KnowledgeParseJobRow {
    id: String,
    document_id: String,
    status: String,
    error_message: Option<String>,
    result_summary: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<KnowledgeParseJobRow> for KnowledgeParseJob {
    fn from(r: KnowledgeParseJobRow) -> Self {
        Self {
            id: r.id,
            document_id: r.document_id,
            status: r.status,
            error_message: r.error_message,
            result_summary: r.result_summary
                .and_then(|s| serde_json::from_str(&s).ok()),
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

// ── Repository trait ──

#[async_trait]
pub trait KnowledgeRepository: Send + Sync {
    // Documents
    async fn list_documents(
        &self, workspace_id: &str, q: Option<&str>,
        tags: Option<&str>, status: Option<&str>,
        page: i64, page_size: i64,
    ) -> Result<(Vec<KnowledgeDocument>, i64), String>;

    async fn get_document(&self, id: &str) -> Result<Option<KnowledgeDocument>, String>;

    async fn create_document(&self, doc: &KnowledgeDocument) -> Result<(), String>;

    async fn update_document(&self, id: &str, title: Option<&str>,
        content: Option<&str>, tags: Option<&str>,
        parse_status: Option<&str>) -> Result<Option<KnowledgeDocument>, String>;

    async fn delete_document(&self, id: &str) -> Result<(), String>;

    // Entities
    async fn list_entities(
        &self, workspace_id: &str, entity_type: Option<&str>,
        tags: Option<&str>,
    ) -> Result<Vec<KnowledgeEntity>, String>;

    async fn get_entity(&self, id: &str) -> Result<Option<KnowledgeEntity>, String>;

    async fn get_entities_by_document(&self, document_id: &str) -> Result<Vec<KnowledgeEntity>, String>;

    async fn upsert_entities(&self, entities: &[KnowledgeEntity]) -> Result<(), String>;

    async fn delete_entities_by_document(&self, document_id: &str) -> Result<(), String>;

    async fn update_entity(&self, id: &str, name: Option<&str>,
        description: Option<&str>, entity_type: Option<&str>,
        properties: Option<&str>, tags: Option<&str>,
        device_id: Option<Option<&str>>) -> Result<Option<KnowledgeEntity>, String>;

    // Relations
    async fn list_relations(&self, workspace_id: &str) -> Result<Vec<KnowledgeRelation>, String>;

    async fn get_relations_by_document(&self, document_id: &str) -> Result<Vec<KnowledgeRelation>, String>;

    async fn upsert_relations(&self, relations: &[KnowledgeRelation]) -> Result<(), String>;

    async fn delete_relations_by_document(&self, document_id: &str) -> Result<(), String>;

    // Search
    async fn search_knowledge(
        &self, workspace_id: &str, query: &str,
        entity_type: Option<&str>, tags: Option<&str>, limit: i64,
    ) -> Result<Vec<KnowledgeSearchResult>, String>;

    // Context: get all parsed entities and relations for Agent injection
    async fn get_context_data(
        &self, workspace_id: &str,
    ) -> Result<(Vec<KnowledgeEntity>, Vec<KnowledgeRelation>), String>;

    // Parse jobs
    async fn create_parse_job(&self, job: &KnowledgeParseJob) -> Result<(), String>;
    async fn get_parse_job(&self, id: &str) -> Result<Option<KnowledgeParseJob>, String>;
    async fn update_parse_job(&self, id: &str, status: &str,
        error_message: Option<&str>,
        result_summary: Option<&str>) -> Result<(), String>;
}

// ── SQLite implementation ──

pub struct SqliteKnowledgeRepository {
    pool: SqlitePool,
}

impl SqliteKnowledgeRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl KnowledgeRepository for SqliteKnowledgeRepository {
    async fn list_documents(
        &self, workspace_id: &str, q: Option<&str>,
        tags: Option<&str>, status: Option<&str>,
        page: i64, page_size: i64,
    ) -> Result<(Vec<KnowledgeDocument>, i64), String> {
        let offset = (page - 1) * page_size;

        let mut where_clauses = vec!["workspace_id = ?".to_string()];
        let mut params: Vec<String> = vec![workspace_id.to_string()];

        if let Some(q) = q {
            where_clauses.push("(title LIKE ? OR content LIKE ?)".to_string());
            params.push(format!("%{}%", q));
            params.push(format!("%{}%", q));
        }
        if let Some(tags) = tags {
            for tag in tags.split(',') {
                where_clauses.push("tags LIKE ?".to_string());
                params.push(format!("%\"{}\"%", tag.trim()));
            }
        }
        if let Some(status) = status {
            where_clauses.push("parse_status = ?".to_string());
            params.push(status.to_string());
        }

        let where_clause = where_clauses.join(" AND ");
        let count_sql = format!("SELECT COUNT(*) FROM knowledge_documents WHERE {}", where_clause);
        let query_sql = format!(
            "SELECT * FROM knowledge_documents WHERE {} ORDER BY updated_at DESC LIMIT ? OFFSET ?",
            where_clause
        );

        let mut count_params = params.clone();
        let mut query_params = params;
        query_params.push(page_size.to_string());
        query_params.push(offset.to_string());

        let total: (i64,) = sqlx::query_as(&count_sql)
            .bind(&count_params[0])
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.to_string())?;

        let rows: Vec<KnowledgeDocumentRow> = sqlx::query_as(&query_sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok((rows.into_iter().map(Into::into).collect(), total.0))
    }

    async fn get_document(&self, id: &str) -> Result<Option<KnowledgeDocument>, String> {
        let row: Option<KnowledgeDocumentRow> = sqlx::query_as(
            "SELECT * FROM knowledge_documents WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(row.map(Into::into))
    }

    async fn create_document(&self, doc: &KnowledgeDocument) -> Result<(), String> {
        sqlx::query(
            "INSERT INTO knowledge_documents (id, workspace_id, title, content, tags, parse_status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&doc.id)
        .bind(&doc.workspace_id)
        .bind(&doc.title)
        .bind(&doc.content)
        .bind(serde_json::to_string(&doc.tags).unwrap_or_default())
        .bind(&doc.parse_status)
        .bind(&doc.created_at)
        .bind(&doc.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn update_document(&self, id: &str, title: Option<&str>,
        content: Option<&str>, tags: Option<&str>,
        parse_status: Option<&str>) -> Result<Option<KnowledgeDocument>, String> {
        let now = chrono::Utc::now().to_rfc3339();

        if let Some(title) = title {
            sqlx::query("UPDATE knowledge_documents SET title = ?, updated_at = ? WHERE id = ?")
                .bind(title).bind(&now).bind(id)
                .execute(&self.pool).await.map_err(|e| e.to_string())?;
        }
        if let Some(content) = content {
            sqlx::query("UPDATE knowledge_documents SET content = ?, updated_at = ? WHERE id = ?")
                .bind(content).bind(&now).bind(id)
                .execute(&self.pool).await.map_err(|e| e.to_string())?;
        }
        if let Some(tags) = tags {
            sqlx::query("UPDATE knowledge_documents SET tags = ?, updated_at = ? WHERE id = ?")
                .bind(tags).bind(&now).bind(id)
                .execute(&self.pool).await.map_err(|e| e.to_string())?;
        }
        if let Some(parse_status) = parse_status {
            sqlx::query("UPDATE knowledge_documents SET parse_status = ?, updated_at = ? WHERE id = ?")
                .bind(parse_status).bind(&now).bind(id)
                .execute(&self.pool).await.map_err(|e| e.to_string())?;
        }

        self.get_document(id).await
    }

    async fn delete_document(&self, id: &str) -> Result<(), String> {
        sqlx::query("DELETE FROM knowledge_relations WHERE source_entity_id IN (SELECT id FROM knowledge_entities WHERE source_document_id = ?)")
            .bind(id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM knowledge_relations WHERE target_entity_id IN (SELECT id FROM knowledge_entities WHERE source_document_id = ?)")
            .bind(id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM knowledge_entities WHERE source_document_id = ?")
            .bind(id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM knowledge_parse_jobs WHERE document_id = ?")
            .bind(id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM knowledge_documents WHERE id = ?")
            .bind(id).execute(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn list_entities(
        &self, workspace_id: &str, entity_type: Option<&str>,
        tags: Option<&str>,
    ) -> Result<Vec<KnowledgeEntity>, String> {
        let mut sql = "SELECT * FROM knowledge_entities WHERE workspace_id = ?".to_string();
        if let Some(t) = entity_type {
            sql.push_str(&format!(" AND entity_type = '{}'", t));
        }
        if let Some(tags) = tags {
            for tag in tags.split(',') {
                sql.push_str(&format!(" AND tags LIKE '%\"{}\"%'", tag.trim()));
            }
        }
        sql.push_str(" ORDER BY name");

        let rows: Vec<KnowledgeEntityRow> = sqlx::query_as(&sql)
            .bind(workspace_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_entity(&self, id: &str) -> Result<Option<KnowledgeEntity>, String> {
        let row: Option<KnowledgeEntityRow> = sqlx::query_as(
            "SELECT * FROM knowledge_entities WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(row.map(Into::into))
    }

    async fn get_entities_by_document(&self, document_id: &str) -> Result<Vec<KnowledgeEntity>, String> {
        let rows: Vec<KnowledgeEntityRow> = sqlx::query_as(
            "SELECT * FROM knowledge_entities WHERE source_document_id = ?"
        )
        .bind(document_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn upsert_entities(&self, entities: &[KnowledgeEntity]) -> Result<(), String> {
        for entity in entities {
            sqlx::query(
                "INSERT OR REPLACE INTO knowledge_entities
                 (id, workspace_id, source_document_id, entity_type, name, description,
                  properties, tags, file_ids, device_id, confidence, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&entity.id)
            .bind(&entity.workspace_id)
            .bind(&entity.source_document_id)
            .bind(&entity.entity_type)
            .bind(&entity.name)
            .bind(&entity.description)
            .bind(serde_json::to_string(&entity.properties).unwrap_or_default())
            .bind(serde_json::to_string(&entity.tags).unwrap_or_default())
            .bind(serde_json::to_string(&entity.file_ids).unwrap_or_default())
            .bind(&entity.device_id)
            .bind(entity.confidence)
            .bind(&entity.created_at)
            .bind(&entity.updated_at)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    async fn delete_entities_by_document(&self, document_id: &str) -> Result<(), String> {
        sqlx::query("DELETE FROM knowledge_entities WHERE source_document_id = ?")
            .bind(document_id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn update_entity(&self, id: &str, name: Option<&str>,
        description: Option<&str>, entity_type: Option<&str>,
        properties: Option<&str>, tags: Option<&str>,
        device_id: Option<Option<&str>>) -> Result<Option<KnowledgeEntity>, String> {
        let now = chrono::Utc::now().to_rfc3339();

        if let Some(name) = name {
            sqlx::query("UPDATE knowledge_entities SET name = ?, updated_at = ? WHERE id = ?")
                .bind(name).bind(&now).bind(id)
                .execute(&self.pool).await.map_err(|e| e.to_string())?;
        }
        if let Some(description) = description {
            sqlx::query("UPDATE knowledge_entities SET description = ?, updated_at = ? WHERE id = ?")
                .bind(description).bind(&now).bind(id)
                .execute(&self.pool).await.map_err(|e| e.to_string())?;
        }
        if let Some(entity_type) = entity_type {
            sqlx::query("UPDATE knowledge_entities SET entity_type = ?, updated_at = ? WHERE id = ?")
                .bind(entity_type).bind(&now).bind(id)
                .execute(&self.pool).await.map_err(|e| e.to_string())?;
        }
        if let Some(properties) = properties {
            sqlx::query("UPDATE knowledge_entities SET properties = ?, updated_at = ? WHERE id = ?")
                .bind(properties).bind(&now).bind(id)
                .execute(&self.pool).await.map_err(|e| e.to_string())?;
        }
        if let Some(tags) = tags {
            sqlx::query("UPDATE knowledge_entities SET tags = ?, updated_at = ? WHERE id = ?")
                .bind(tags).bind(&now).bind(id)
                .execute(&self.pool).await.map_err(|e| e.to_string())?;
        }
        if let Some(device_id) = device_id {
            sqlx::query("UPDATE knowledge_entities SET device_id = ?, updated_at = ? WHERE id = ?")
                .bind(device_id).bind(&now).bind(id)
                .execute(&self.pool).await.map_err(|e| e.to_string())?;
        }

        self.get_entity(id).await
    }

    async fn list_relations(&self, workspace_id: &str) -> Result<Vec<KnowledgeRelation>, String> {
        let rows: Vec<KnowledgeRelationRow> = sqlx::query_as(
            "SELECT * FROM knowledge_relations WHERE workspace_id = ?"
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_relations_by_document(&self, document_id: &str) -> Result<Vec<KnowledgeRelation>, String> {
        let rows: Vec<KnowledgeRelationRow> = sqlx::query_as(
            "SELECT kr.* FROM knowledge_relations kr
             JOIN knowledge_entities ke ON kr.source_entity_id = ke.id
             WHERE ke.source_document_id = ?"
        )
        .bind(document_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn upsert_relations(&self, relations: &[KnowledgeRelation]) -> Result<(), String> {
        for rel in relations {
            sqlx::query(
                "INSERT OR REPLACE INTO knowledge_relations
                 (id, workspace_id, source_entity_id, target_entity_id,
                  relation_type, properties, confidence)
                 VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&rel.id)
            .bind(&rel.workspace_id)
            .bind(&rel.source_entity_id)
            .bind(&rel.target_entity_id)
            .bind(&rel.relation_type)
            .bind(serde_json::to_string(&rel.properties).unwrap_or_default())
            .bind(rel.confidence)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    async fn delete_relations_by_document(&self, document_id: &str) -> Result<(), String> {
        sqlx::query(
            "DELETE FROM knowledge_relations WHERE source_entity_id IN
             (SELECT id FROM knowledge_entities WHERE source_document_id = ?)"
        )
        .bind(document_id)
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn search_knowledge(
        &self, workspace_id: &str, query: &str,
        entity_type: Option<&str>, tags: Option<&str>, limit: i64,
    ) -> Result<Vec<KnowledgeSearchResult>, String> {
        let mut sql = "SELECT * FROM knowledge_entities WHERE workspace_id = ?
                       AND (name LIKE ? OR description LIKE ?)".to_string();

        if let Some(t) = entity_type {
            sql.push_str(&format!(" AND entity_type = '{}'", t));
        }
        if let Some(tags) = tags {
            for tag in tags.split(',') {
                sql.push_str(&format!(" AND tags LIKE '%\"{}\"%'", tag.trim()));
            }
        }
        sql.push_str(&format!(" ORDER BY confidence DESC LIMIT {}", limit));

        let like = format!("%{}%", query);
        let rows: Vec<KnowledgeEntityRow> = sqlx::query_as(&sql)
            .bind(workspace_id)
            .bind(&like)
            .bind(&like)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?;

        let mut results = Vec::new();
        for row in rows {
            let entity: KnowledgeEntity = row.into();
            let rel_rows: Vec<KnowledgeRelationRow> = sqlx::query_as(
                "SELECT * FROM knowledge_relations WHERE source_entity_id = ? OR target_entity_id = ?"
            )
            .bind(&entity.id)
            .bind(&entity.id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| e.to_string())?;

            let relations: Vec<KnowledgeRelation> = rel_rows.into_iter().map(Into::into).collect();

            let doc: Option<KnowledgeDocumentRow> = sqlx::query_as(
                "SELECT * FROM knowledge_documents WHERE id = ?"
            )
            .bind(&entity.source_document_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| e.to_string())?;

            let snippet = doc.map(|d| {
                d.content.chars().take(500).collect()
            }).unwrap_or_default();

            let relevance = if entity.name.to_lowercase().contains(&query.to_lowercase()) { 0.9 }
                else { 0.5 };

            results.push(KnowledgeSearchResult {
                entity,
                relations,
                source_snippet: snippet,
                relevance,
            });
        }

        Ok(results)
    }

    async fn get_context_data(
        &self, workspace_id: &str,
    ) -> Result<(Vec<KnowledgeEntity>, Vec<KnowledgeRelation>), String> {
        let entities = self.list_entities(workspace_id, None, None).await?;
        let relations = self.list_relations(workspace_id).await?;
        Ok((entities, relations))
    }

    async fn create_parse_job(&self, job: &KnowledgeParseJob) -> Result<(), String> {
        sqlx::query(
            "INSERT INTO knowledge_parse_jobs (id, document_id, status, error_message, result_summary, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&job.id)
        .bind(&job.document_id)
        .bind(&job.status)
        .bind(&job.error_message)
        .bind(job.result_summary.as_ref().map(|s| serde_json::to_string(s).unwrap_or_default()))
        .bind(&job.created_at)
        .bind(&job.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn get_parse_job(&self, id: &str) -> Result<Option<KnowledgeParseJob>, String> {
        let row: Option<KnowledgeParseJobRow> = sqlx::query_as(
            "SELECT * FROM knowledge_parse_jobs WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;

        Ok(row.map(Into::into))
    }

    async fn update_parse_job(&self, id: &str, status: &str,
        error_message: Option<&str>,
        result_summary: Option<&str>) -> Result<(), String> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE knowledge_parse_jobs SET status = ?, error_message = ?, result_summary = ?, updated_at = ? WHERE id = ?"
        )
        .bind(status)
        .bind(error_message)
        .bind(result_summary)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
```

- [ ] **Step 3: Update mod.rs to re-export repository**

Ensure `cloud/src/modules/workspace/mod.rs` has:
```rust
pub use repo::knowledge::KnowledgeRepository;
```

- [ ] **Step 4: Build to verify compilation**

Run: `cargo build`
Expected: Repository trait and implementation compile without errors

- [ ] **Step 5: Commit**

```bash
git add cloud/src/modules/workspace/repo/knowledge.rs cloud/src/modules/workspace/mod.rs
git commit -m "feat: add knowledge repository trait and SQLite implementation"
```

---

### Task 4: KnowledgeService (Backend)

**Files:**
- Create: `cloud/src/modules/workspace/service/knowledge.rs`
- Modify: `cloud/src/modules/workspace/mod.rs`

- [ ] **Step 1: Read AppState and existing service pattern**

Read `cloud/src/shared/app_state.rs` to see how AppState holds services.
Read `cloud/src/modules/workspace/service.rs` for the WorkspaceService constructor pattern.

- [ ] **Step 2: Read MiniMax/zeroclaw config for LLM integration**

Run: `grep -r "zeroclaw" cloud/src/shared/ --include="*.rs" -l`
Read the config file to understand the MiniMax config structure.

- [ ] **Step 3: Write KnowledgeService**

```rust
// cloud/src/modules/workspace/service/knowledge.rs

use std::sync::Arc;
use chrono::Utc;
use crate::shared::{id, config};
use super::super::types::knowledge::*;
use super::super::repo::knowledge::KnowledgeRepository;

pub struct KnowledgeService {
    repo: Arc<dyn KnowledgeRepository>,
}

impl KnowledgeService {
    pub fn new(repo: Arc<dyn KnowledgeRepository>) -> Self {
        Self { repo }
    }

    // ── Document CRUD ──

    pub async fn list_documents(
        &self, workspace_id: &str, q: Option<&str>,
        tags: Option<&str>, status: Option<&str>,
        page: i64, page_size: i64,
    ) -> Result<(Vec<KnowledgeDocument>, i64), String> {
        self.repo.list_documents(workspace_id, q, tags, status, page, page_size).await
    }

    pub async fn get_document(&self, id: &str) -> Result<Option<KnowledgeDocument>, String> {
        self.repo.get_document(id).await
    }

    pub async fn create_document(
        &self, workspace_id: &str, title: &str, content: &str,
        tags: &[String], file_ids: &[String],
    ) -> Result<KnowledgeDocument, String> {
        let now = Utc::now().to_rfc3339();
        let doc = KnowledgeDocument {
            id: id::new(),
            workspace_id: workspace_id.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            tags: tags.to_vec(),
            parse_status: "pending".to_string(),
            created_at: now.clone(),
            updated_at: now,
        };
        self.repo.create_document(&doc).await?;
        Ok(doc)
    }

    pub async fn update_document(
        &self, id: &str, title: Option<&str>, content: Option<&str>,
        tags: Option<&[String]>,
    ) -> Result<Option<KnowledgeDocument>, String> {
        let tags_json = tags.map(|t| serde_json::to_string(&t).unwrap_or_default());

        let result = self.repo.update_document(
            id, title, content, tags_json.as_deref(),
            if content.is_some() { Some("pending") } else { None },
        ).await?;

        Ok(result)
    }

    pub async fn delete_document(&self, id: &str) -> Result<(), String> {
        self.repo.delete_document(id).await
    }

    // ── Entity management ──

    pub async fn list_entities(
        &self, workspace_id: &str, entity_type: Option<&str>,
        tags: Option<&str>,
    ) -> Result<Vec<KnowledgeEntity>, String> {
        self.repo.list_entities(workspace_id, entity_type, tags).await
    }

    pub async fn update_entity(
        &self, id: &str, req: &UpdateKnowledgeEntityRequest,
    ) -> Result<Option<KnowledgeEntity>, String> {
        let properties = req.properties.as_ref()
            .map(|p| serde_json::to_string(p).unwrap_or_default());
        let tags = req.tags.as_ref()
            .map(|t| serde_json::to_string(t).unwrap_or_default());

        self.repo.update_entity(
            id, req.name.as_deref(), req.description.as_deref(),
            req.entity_type.as_deref(), properties.as_deref(),
            tags.as_deref(), req.device_id.as_deref(),
        ).await
    }

    // ── Parse pipeline ──

    pub async fn trigger_parse(&self, document_id: &str, workspace_id: &str) -> Result<String, String> {
        let now = Utc::now().to_rfc3339();
        let parse_id = id::new();

        let job = KnowledgeParseJob {
            id: parse_id.clone(),
            document_id: document_id.to_string(),
            status: "pending".to_string(),
            error_message: None,
            result_summary: None,
            created_at: now.clone(),
            updated_at: now,
        };
        self.repo.create_parse_job(&job).await?;

        // Spawn background parse task
        let repo = self.repo.clone();
        let did = document_id.to_string();
        let wid = workspace_id.to_string();
        let pid = parse_id.clone();

        tokio::spawn(async move {
            Self::run_parse(repo, &did, &wid, &pid).await;
        });

        Ok(parse_id)
    }

    async fn run_parse(
        repo: Arc<dyn KnowledgeRepository>, document_id: &str,
        workspace_id: &str, parse_id: &str,
    ) {
        // Mark running
        let _ = repo.update_parse_job(parse_id, "running", None, None).await;
        let _ = repo.update_document(document_id, None, None, None, Some("parsing")).await;

        let doc = match repo.get_document(document_id).await {
            Ok(Some(d)) => d,
            _ => {
                let _ = repo.update_parse_job(parse_id, "failed", Some("文档不存在"), None).await;
                return;
            }
        };

        match Self::call_llm_parse(&doc.content).await {
            Ok((entities, relations)) => {
                // Get previous entities for diff
                let prev_entities = repo.get_entities_by_document(document_id).await.unwrap_or_default();
                let prev_relations = repo.get_relations_by_document(document_id).await.unwrap_or_default();

                // Replace entities and relations for this document
                let _ = repo.delete_relations_by_document(document_id).await;
                let _ = repo.delete_entities_by_document(document_id).await;
                let _ = repo.upsert_entities(&entities).await;
                let _ = repo.upsert_relations(&relations).await;

                // Compute diff
                let diff = ParseDiff {
                    added: entities.len().saturating_sub(prev_entities.len()),
                    removed: 0,
                    modified: (entities.len() != prev_entities.len()) as usize,
                };

                let summary = ParseResultSummary {
                    entity_count: entities.len(),
                    relation_count: relations.len(),
                    diff: Some(diff),
                };
                let summary_json = serde_json::to_string(&summary).unwrap_or_default();

                let _ = repo.update_parse_job(parse_id, "completed", None, Some(&summary_json)).await;
                let _ = repo.update_document(document_id, None, None, None, Some("parsed")).await;

                // Generate and save tags for the document
                if let Ok(tags) = Self::generate_tags(&doc.content).await {
                    let tags_json = serde_json::to_string(&tags).unwrap_or_default();
                    let _ = repo.update_document(document_id, None, None, Some(&tags_json), None).await;
                }
            }
            Err(e) => {
                let _ = repo.update_parse_job(parse_id, "failed", Some(&e), None).await;
                let _ = repo.update_document(document_id, None, None, None, Some("failed")).await;
            }
        }
    }

    async fn call_llm_parse(content: &str) -> Result<(Vec<KnowledgeEntity>, Vec<KnowledgeRelation>), String> {
        let config = config::get();
        let minimax = config.minimax.as_ref().ok_or("AI 服务未配置")?;
        let auth_token = &minimax.auth_token;
        let model = minimax.model.as_deref().unwrap_or("minimax-m2");

        let prompt = format!(
            r#"你是一个知识图谱提取助手。从以下用户文档中提取实体和关系。

<user_document>
{}
</user_document>

实体类型定义：
- space：空间场所（建筑、楼层、机房、园区）
- device：设备/传感器（网关、传感器、控制器）
- functional：功能要素（消防、供电、安防）
- custom:xxx：自定义类型

关系类型定义：
- contains：空间包含空间/设备
- manages：网关管理终端
- monitors：传感器监控空间
- references：实体引用文档
- connects_to：设备连接设备
- custom:xxx：自定义关系

标签规范：使用简短中文标签，如 #机房 #温湿度 #安防 #网关 #消防

请以 JSON 格式返回，格式如下：
{{
  "entities": [
    {{
      "id": "e-001",
      "entity_type": "space",
      "name": "机房A",
      "description": "3号楼数据中心机房",
      "properties": {{}},
      "tags": ["机房", "数据中心"],
      "confidence": 0.95
    }}
  ],
  "relations": [
    {{
      "id": "r-001",
      "source_entity_id": "e-001",
      "target_entity_id": "e-002",
      "relation_type": "contains",
      "properties": {{}},
      "confidence": 0.9
    }}
  ]
}}

严格只返回 JSON，不要任何解释或额外文字。如果文档中没有有效实体，返回空的 entities 和 relations 数组。"#,
            content
        );

        let provider = zeroclaw::providers::create_provider("minimaxi", Some(auth_token))
            .map_err(|e| format!("AI provider init failed: {}", e))?;

        let response = retry_llm_call(|| async {
            provider.chat_with_system(None, &prompt, model, Some(0.1)).await
        })
        .await?;

        let json_str = response.trim();
        let parsed: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| format!("LLM 返回格式异常: {}", e))?;

        let entities = parse_entities_from_json(&parsed, content);
        let relations = parse_relations_from_json(&parsed);

        Ok((entities, relations))
    }

    pub async fn preview_parse(
        content: &str, workspace_id: &str,
    ) -> Result<PreviewParseResponse, String> {
        let config = config::get();
        let minimax = config.minimax.as_ref().ok_or("AI 服务未配置")?;
        let auth_token = &minimax.auth_token;
        let model = minimax.model.as_deref().unwrap_or("minimax-m2");

        let prompt = format!(
            r#"你是一个知识图谱提取助手。从以下用户文档中快速提取实体和关系，不做深度推理。

<user_document>
{}
</user_document>

实体类型：space(空间), device(设备), functional(功能), custom:xxx
关系类型：contains, manages, monitors, references, connects_to, custom:xxx

返回 JSON：
{{
  "entities": [{{"id":"e-001","entity_type":"space","name":"...","description":"...","properties":{{}},"tags":["..."],"confidence":0.9}}],
  "relations": [{{"id":"r-001","source_entity_id":"e-001","target_entity_id":"e-002","relation_type":"contains","properties":{{}},"confidence":0.8}}]
}}

只返回 JSON。#         "#,
            content
        );

        let provider = zeroclaw::providers::create_provider("minimaxi", Some(auth_token))
            .map_err(|e| format!("AI provider init failed: {}", e))?;

        let response = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            provider.chat_with_system(None, &prompt, model, Some(0.1)),
        )
        .await
        .map_err(|_| "preview timeout".to_string())?
        .map_err(|e| format!("preview failed: {}", e))?;

        let parsed: serde_json::Value = serde_json::from_str(response.trim())
            .map_err(|e| format!("preview parse: {}", e))?;

        let mut entities = parse_entities_from_json(&parsed, content);
        let relations = parse_relations_from_json(&parsed);

        // Set workspace_id and source_document_id to preview placeholders
        for e in &mut entities {
            e.workspace_id = workspace_id.to_string();
            e.source_document_id = "preview".to_string();
        }

        Ok(PreviewParseResponse { entities, relations })
    }

    // ── Context generation for Agent injection ──

    pub async fn build_context(&self, workspace_id: &str) -> String {
        let (entities, relations) = match self.repo.get_context_data(workspace_id).await {
            Ok(data) => data,
            Err(_) => return String::new(),
        };

        if entities.is_empty() {
            return String::new();
        }

        let mut ctx = String::new();
        ctx.push_str("[工作区知识上下文]\n");

        // Build tree from contains relations
        ctx.push_str(&Self::build_tree(&entities, &relations));
        ctx
    }

    fn build_tree(entities: &[KnowledgeEntity], relations: &[KnowledgeRelation]) -> String {
        // Find root entities (those not contained by any other)
        let contained: std::collections::HashSet<&str> = relations
            .iter()
            .filter(|r| r.relation_type == "contains")
            .map(|r| r.target_entity_id.as_str())
            .collect();

        let roots: Vec<&KnowledgeEntity> = entities
            .iter()
            .filter(|e| !contained.contains(e.id.as_str()))
            .collect();

        let mut tree = String::new();
        for root in roots {
            Self::render_tree_node(root, entities, relations, &mut tree, "");
        }
        tree
    }

    fn render_tree_node(
        entity: &KnowledgeEntity, entities: &[KnowledgeEntity],
        relations: &[KnowledgeRelation], output: &mut String, prefix: &str,
    ) {
        let type_icon = match entity.entity_type.as_str() {
            "space" => "  ",
            "device" => "  ",
            "functional" => "  ",
            _ => "  ",
        };
        let desc = entity.description.as_deref().unwrap_or("");
        output.push_str(&format!("{}{}{} [{}]\n", prefix, type_icon, entity.name, entity.entity_type));
        if !desc.is_empty() {
            output.push_str(&format!("{}   {}\n", prefix, desc));
        }

        // Find children via contains relation
        let children: Vec<&str> = relations
            .iter()
            .filter(|r| r.relation_type == "contains" && r.source_entity_id == entity.id)
            .map(|r| r.target_entity_id.as_str())
            .collect();

        for (i, child_id) in children.iter().enumerate() {
            if let Some(child) = entities.iter().find(|e| e.id == *child_id) {
                let is_last = i == children.len() - 1;
                let child_prefix = if is_last {
                    format!("{}└── ", prefix)
                } else {
                    format!("{}├── ", prefix)
                };
                let next_prefix = if is_last {
                    format!("{}    ", prefix)
                } else {
                    format!("{}│   ", prefix)
                };
                let type_icon = match child.entity_type.as_str() {
                    "space" => "  ",
                    "device" => "  ",
                    _ => "  ",
                };
                let desc = child.description.as_deref().unwrap_or("");
                output.push_str(&format!("{}{}{} [{}]\n", child_prefix, type_icon, child.name, child.entity_type));
                if !desc.is_empty() {
                    output.push_str(&format!("{}   {}\n", next_prefix, desc));
                }
            }
        }
    }

    // ── Parse job status ──

    pub async fn get_parse_job(&self, id: &str) -> Result<Option<KnowledgeParseJob>, String> {
        self.repo.get_parse_job(id).await
    }

    // ── Search ──

    pub async fn search_knowledge(
        &self, workspace_id: &str, query: &str,
        entity_type: Option<&str>, tags: Option<&str>, limit: i64,
    ) -> Result<Vec<KnowledgeSearchResult>, String> {
        self.repo.search_knowledge(workspace_id, query, entity_type, tags, limit).await
    }

    // ── Tag generation ──

    async fn generate_tags(content: &str) -> Result<Vec<String>, String> {
        let config = config::get();
        let minimax = match config.minimax.as_ref() {
            Some(m) => m,
            None => return Ok(vec![]),
        };

        let prompt = format!(
            "为以下文档生成 3-5 个简洁的中文标签，只返回逗号分隔的标签，不要任何解释。\n\n文档内容：\n{}",
            &content[..content.len().min(2000)]
        );

        let provider = zeroclaw::providers::create_provider("minimaxi", Some(&minimax.auth_token))
            .map_err(|_| "tag provider init failed".to_string())?;

        let response = provider
            .chat_with_system(None, &prompt, minimax.model.as_deref().unwrap_or("minimax-m2"), Some(0.3))
            .await
            .map_err(|e| format!("tag generation failed: {}", e))?;

        let tags: Vec<String> = response
            .split([',', '，', '、', '\n'])
            .map(|t| t.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|t| !t.is_empty() && t.len() < 20)
            .collect();

        Ok(tags)
    }
}

// ── Helper functions ──

async fn retry_llm_call<F, Fut>(f: F) -> Result<String, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    let mut last_err = String::new();
    for attempt in 0..3 {
        match tokio::time::timeout(std::time::Duration::from_secs(10), f()).await {
            Ok(Ok(response)) => return Ok(response),
            Ok(Err(e)) => {
                last_err = e;
                if attempt < 2 {
                    tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
                }
            }
            Err(_) => {
                last_err = "AI 解析超时".to_string();
                if attempt < 2 {
                    tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
                }
            }
        }
    }
    Err(last_err)
}

fn parse_entities_from_json(parsed: &serde_json::Value, _source_content: &str) -> Vec<KnowledgeEntity> {
    let now = Utc::now().to_rfc3339();
    let mut entities = Vec::new();

    if let Some(arr) = parsed["entities"].as_array() {
        for item in arr {
            let id = item["id"].as_str().map(|s| s.to_string()).unwrap_or_else(id::new);
            let entity_type = item["entity_type"].as_str().unwrap_or("custom:unknown").to_string();
            let name = item["name"].as_str().unwrap_or("未命名实体").to_string();
            let description = item["description"].as_str().map(|s| s.to_string());
            let props = item["properties"].clone();
            let tags: Vec<String> = item["tags"].as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let confidence = item["confidence"].as_f64().unwrap_or(0.5) as f32;

            entities.push(KnowledgeEntity {
                id,
                workspace_id: String::new(), // filled by caller
                source_document_id: String::new(), // filled by caller
                entity_type,
                name,
                description,
                properties: props,
                tags,
                file_ids: vec![],
                device_id: None,
                confidence,
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        }
    }
    entities
}

fn parse_relations_from_json(parsed: &serde_json::Value) -> Vec<KnowledgeRelation> {
    let mut relations = Vec::new();

    if let Some(arr) = parsed["relations"].as_array() {
        for item in arr {
            let id = item["id"].as_str().map(|s| s.to_string()).unwrap_or_else(id::new);
            let source = match item["source_entity_id"].as_str() {
                Some(s) => s.to_string(),
                None => continue,
            };
            let target = match item["target_entity_id"].as_str() {
                Some(s) => s.to_string(),
                None => continue,
            };
            let rel_type = item["relation_type"].as_str().unwrap_or("custom:unknown").to_string();
            let props = item["properties"].clone();
            let confidence = item["confidence"].as_f64().unwrap_or(0.5) as f32;

            relations.push(KnowledgeRelation {
                id,
                workspace_id: String::new(),
                source_entity_id: source,
                target_entity_id: target,
                relation_type: rel_type,
                properties: props,
                confidence,
            });
        }
    }
    relations
}
```

- [ ] **Step 4: Build to verify compilation**

Run: `cargo build`
Expected: KnowledgeService compiles without errors

- [ ] **Step 5: Commit**

```bash
git add cloud/src/modules/workspace/service/knowledge.rs
git commit -m "feat: add KnowledgeService with parse pipeline, context generation, and search"
```

---

### Task 5: Knowledge Handler Routes (Backend)

**Files:**
- Create: `cloud/src/modules/workspace/handler/knowledge.rs`
- Modify: `cloud/src/modules/workspace/handler.rs`
- Modify: `cloud/src/modules/workspace/mod.rs`

- [ ] **Step 1: Read the existing router mounting pattern**

Read `cloud/src/modules/workspace/handler.rs` to see how the main router is structured and how to nest a sub-router.

- [ ] **Step 2: Write knowledge handler**

```rust
// cloud/src/modules/workspace/handler/knowledge.rs

use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    routing::{delete, get, post, put},
};
use tinyiothub_web::response::ApiResponseBuilder;

use super::super::types::knowledge::*;
use crate::shared::{api_response::ApiResponse, app_state::AppState, security::jwt::Claims};

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/documents", get(list_documents))
        .route("/documents", post(create_document))
        .route("/documents/{did}", get(get_document))
        .route("/documents/{did}", put(update_document))
        .route("/documents/{did}", delete(delete_document))
        .route("/documents/{did}/parse", post(trigger_parse))
        .route("/documents/{did}/preview", post(preview_parse))
        .route("/parse/{job_id}", get(get_parse_job))
        .route("/entities", get(list_entities))
        .route("/entities/{eid}", put(update_entity))
        .route("/relations", get(list_relations))
        .route("/search", get(search_knowledge))
        .route("/context", get(get_context))
}

async fn list_documents(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Query(params): Query<KnowledgeDocumentListParams>,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.knowledge_service.list_documents(
        &workspace_id,
        params.q.as_deref(),
        params.tags.as_deref(),
        params.status.as_deref(),
        params.page.unwrap_or(1),
        params.page_size.unwrap_or(20),
    ).await {
        Ok((docs, total)) => ApiResponseBuilder::success(serde_json::json!({
            "data": docs,
            "total": total,
            "page": params.page.unwrap_or(1),
            "page_size": params.page_size.unwrap_or(20),
        })),
        Err(e) => {
            tracing::error!("Failed to list knowledge documents: {}", e);
            ApiResponseBuilder::error("获取知识文档列表失败")
        }
    }
}

async fn create_document(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Json(payload): Json<CreateKnowledgeDocumentRequest>,
) -> Json<ApiResponse<KnowledgeDocument>> {
    match state.knowledge_service.create_document(
        &workspace_id,
        &payload.title,
        &payload.content,
        &payload.tags.unwrap_or_default(),
        &payload.file_ids.unwrap_or_default(),
    ).await {
        Ok(doc) => ApiResponseBuilder::success(doc),
        Err(e) => {
            tracing::error!("Failed to create knowledge document: {}", e);
            ApiResponseBuilder::error("创建知识文档失败")
        }
    }
}

async fn get_document(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path((_workspace_id, doc_id)): Path<(String, String)>,
) -> Json<ApiResponse<KnowledgeDocument>> {
    match state.knowledge_service.get_document(&doc_id).await {
        Ok(Some(doc)) => ApiResponseBuilder::success(doc),
        Ok(None) => ApiResponseBuilder::error_with_code(404, "文档不存在"),
        Err(e) => {
            tracing::error!("Failed to get knowledge document: {}", e);
            ApiResponseBuilder::error("获取知识文档失败")
        }
    }
}

async fn update_document(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path((_workspace_id, doc_id)): Path<(String, String)>,
    Json(payload): Json<UpdateKnowledgeDocumentRequest>,
) -> Json<ApiResponse<KnowledgeDocument>> {
    match state.knowledge_service.update_document(
        &doc_id,
        payload.title.as_deref(),
        payload.content.as_deref(),
        payload.tags.as_deref(),
    ).await {
        Ok(Some(doc)) => ApiResponseBuilder::success(doc),
        Ok(None) => ApiResponseBuilder::error_with_code(404, "文档不存在"),
        Err(e) => {
            tracing::error!("Failed to update knowledge document: {}", e);
            ApiResponseBuilder::error("更新知识文档失败")
        }
    }
}

async fn delete_document(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path((_workspace_id, doc_id)): Path<(String, String)>,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.knowledge_service.delete_document(&doc_id).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"success": true})),
        Err(e) => {
            tracing::error!("Failed to delete knowledge document: {}", e);
            ApiResponseBuilder::error("删除知识文档失败")
        }
    }
}

async fn trigger_parse(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path((workspace_id, doc_id)): Path<(String, String)>,
) -> Json<ApiResponse<serde_json::Value>> {
    match state.knowledge_service.trigger_parse(&doc_id, &workspace_id).await {
        Ok(parse_id) => ApiResponseBuilder::success(serde_json::json!({"parse_id": parse_id})),
        Err(e) => {
            tracing::error!("Failed to trigger parse: {}", e);
            ApiResponseBuilder::error("触发解析失败")
        }
    }
}

async fn preview_parse(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path((workspace_id, _doc_id)): Path<(String, String)>,
    Json(payload): Json<PreviewParseRequest>,
) -> Json<ApiResponse<PreviewParseResponse>> {
    match KnowledgeService::preview_parse(&payload.content, &workspace_id).await {
        Ok(result) => ApiResponseBuilder::success(result),
        Err(e) => {
            tracing::error!("Preview parse failed: {}", e);
            ApiResponseBuilder::error("预览解析失败")
        }
    }
}

async fn get_parse_job(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path((_workspace_id, job_id)): Path<(String, String)>,
) -> Json<ApiResponse<KnowledgeParseJob>> {
    match state.knowledge_service.get_parse_job(&job_id).await {
        Ok(Some(job)) => ApiResponseBuilder::success(job),
        Ok(None) => ApiResponseBuilder::error_with_code(404, "解析任务不存在"),
        Err(e) => {
            tracing::error!("Failed to get parse job: {}", e);
            ApiResponseBuilder::error("获取解析任务失败")
        }
    }
}

async fn list_entities(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Query(params): Query<KnowledgeEntityListParams>,
) -> Json<ApiResponse<Vec<KnowledgeEntity>>> {
    match state.knowledge_service.list_entities(
        &workspace_id, params.entity_type.as_deref(), params.tags.as_deref(),
    ).await {
        Ok(entities) => ApiResponseBuilder::success(entities),
        Err(e) => {
            tracing::error!("Failed to list entities: {}", e);
            ApiResponseBuilder::error("获取实体列表失败")
        }
    }
}

async fn update_entity(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path((_workspace_id, entity_id)): Path<(String, String)>,
    Json(payload): Json<UpdateKnowledgeEntityRequest>,
) -> Json<ApiResponse<KnowledgeEntity>> {
    match state.knowledge_service.update_entity(&entity_id, &payload).await {
        Ok(Some(entity)) => ApiResponseBuilder::success(entity),
        Ok(None) => ApiResponseBuilder::error_with_code(404, "实体不存在"),
        Err(e) => {
            tracing::error!("Failed to update entity: {}", e);
            ApiResponseBuilder::error("更新实体失败")
        }
    }
}

async fn list_relations(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> Json<ApiResponse<Vec<KnowledgeRelation>>> {
    match state.knowledge_service.list_relations(&workspace_id).await {
        Ok(relations) => ApiResponseBuilder::success(relations),
        Err(e) => {
            tracing::error!("Failed to list relations: {}", e);
            ApiResponseBuilder::error("获取关系列表失败")
        }
    }
}

async fn search_knowledge(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
    Query(params): Query<KnowledgeSearchParams>,
) -> Json<ApiResponse<Vec<KnowledgeSearchResult>>> {
    if params.q.is_empty() {
        return ApiResponseBuilder::error_with_code(400, "搜索关键词不能为空");
    }
    match state.knowledge_service.search_knowledge(
        &workspace_id,
        &params.q,
        params.entity_type.as_deref(),
        params.tags.as_deref(),
        params.limit.unwrap_or(10).clamp(1, 50),
    ).await {
        Ok(results) => ApiResponseBuilder::success(results),
        Err(e) => {
            tracing::error!("Failed to search knowledge: {}", e);
            ApiResponseBuilder::error("搜索知识失败")
        }
    }
}

async fn get_context(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(workspace_id): Path<String>,
) -> String {
    state.knowledge_service.build_context(&workspace_id).await
}
```

- [ ] **Step 3: Mount knowledge routes in main handler router**

In `cloud/src/modules/workspace/handler.rs`, add the knowledge routes nesting. Locate the `create_router()` function and add:

```rust
use super::handler::knowledge;

// Inside create_router(), add after the existing routes:
.route("/{id}/knowledge", knowledge::create_router())
```

Note: Because knowledge routes start with `/documents`, `/entities`, etc. (not `/{id}/...` prefix), the sub-router should be mounted differently. Update the router:

```rust
pub fn create_router() -> Router<AppState> {
    // Knowledge routes use {id} from parent scope
    // Mount knowledge sub-router WITH {id} prefix
    // Actually, nest under a path that includes the id
    let knowledge_router = knowledge::create_router();

    Router::new()
        .route("/", get(list_workspaces))
        .route("/", post(create_workspace))
        .route("/{id}", get(get_workspace))
        .route("/{id}", put(update_workspace))
        .route("/{id}", delete(delete_workspace))
        .route("/{id}/devices", post(assign_device))
        .route("/{id}/resources", get(list_resources))
        .route("/{id}/resources", post(create_resource))
        .route("/{id}/resources/suggest-tags", post(suggest_tags))
        .route("/{id}/resources/search", get(search_resources))
        .route("/{id}/resources/{rid}", get(get_resource))
        .route("/{id}/resources/{rid}", put(update_resource))
        .route("/{id}/resources/{rid}", delete(delete_resource))
        .nest("/{id}/knowledge", knowledge_router) // NEW
}
```

Wait — the knowledge handler's `Path` extractors already include `workspace_id` in some handlers. Let's look at the handler signatures more carefully. Actually, the knowledge handler handlers use `Path((workspace_id, ...))` pattern. But if we nest with `/{id}/knowledge`, then the `{id}` becomes the workspace_id. The handlers use `Path(workspace_id): Path<String>` for some and `Path((workspace_id, doc_id))` for others.

This is correct — Axum nesting means `/{id}/knowledge/documents/{did}` will match `Path((workspace_id, doc_id)): Path<(String, String)>` in the nested handler. But wait — actually with `nest`, the path parameters from the parent are NOT automatically passed to the child. The child router only sees the path after `/knowledge/`.

So we need to adjust the knowledge handler paths: each handler must include `workspace_id` as its first path parameter. Let me fix:

The nested route `/{id}/knowledge` with child route `/documents/{did}` results in URL `/{id}/knowledge/documents/{did}`. But with `nest`, the child only sees `/documents/{did}` — the `{id}` is consumed by the parent. So we need a different approach.

Let's use `nest_service` or just add routes directly without nesting. The simpler approach: add knowledge routes directly to the main router:

```rust
// In create_router():
.route("/{id}/knowledge/documents", get(knowledge::list_documents))
.route("/{id}/knowledge/documents", post(knowledge::create_document))
.route("/{id}/knowledge/documents/{did}", get(knowledge::get_document))
.route("/{id}/knowledge/documents/{did}", put(knowledge::update_document))
.route("/{id}/knowledge/documents/{did}", delete(knowledge::delete_document))
.route("/{id}/knowledge/documents/{did}/parse", post(knowledge::trigger_parse))
.route("/{id}/knowledge/documents/{did}/preview", post(knowledge::preview_parse))
.route("/{id}/knowledge/parse/{job_id}", get(knowledge::get_parse_job))
.route("/{id}/knowledge/entities", get(knowledge::list_entities))
.route("/{id}/knowledge/entities/{eid}", put(knowledge::update_entity))
.route("/{id}/knowledge/relations", get(knowledge::list_relations))
.route("/{id}/knowledge/search", get(knowledge::search_knowledge))
.route("/{id}/knowledge/context", get(knowledge::get_context))
```

This is the safest approach, matching the existing pattern where all routes are declared flat.

- [ ] **Step 4: Add KnowledgeService to AppState**

Read `cloud/src/shared/app_state.rs`, add:
```rust
pub knowledge_service: KnowledgeService,
```

And update the AppState constructor to initialize it.

- [ ] **Step 5: Build to verify all routes compile**

Run: `cargo build`
Expected: No compilation errors

- [ ] **Step 6: Commit**

```bash
git add cloud/src/modules/workspace/handler/knowledge.rs cloud/src/modules/workspace/handler.rs cloud/src/shared/app_state.rs
git commit -m "feat: add knowledge handler routes"
```

---

### Task 6: Agent search_knowledge Tool

**Files:**
- Create: `cloud/src/modules/agent/tools/knowledge.rs`
- Modify: `cloud/src/modules/agent/service.rs` (inject knowledge context)

- [ ] **Step 1: Read existing tool registration pattern**

Read `cloud/src/modules/agent/tools/service.rs` to see how `search_workspace_resources` tool is registered — follow the same pattern for `search_knowledge`.

- [ ] **Step 2: Write search_knowledge tool**

```rust
// cloud/src/modules/agent/tools/knowledge.rs

use serde::{Deserialize, Serialize};
use crate::shared::app_state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchKnowledgeParams {
    pub query: String,
    pub entity_type: Option<String>,
    pub tags: Option<Vec<String>>,
    pub limit: Option<i32>,
}

pub async fn search_knowledge(
    state: &AppState,
    workspace_id: &str,
    params: SearchKnowledgeParams,
) -> Result<String, String> {
    let tags_str = params.tags.as_ref().map(|t| t.join(","));

    let results = state.knowledge_service.search_knowledge(
        workspace_id,
        &params.query,
        params.entity_type.as_deref(),
        tags_str.as_deref(),
        params.limit.unwrap_or(10).clamp(1, 50) as i64,
    ).await?;

    if results.is_empty() {
        return Ok("未找到相关知识图谱信息。".to_string());
    }

    let mut output = String::from("知识图谱搜索结果：\n\n");
    for (i, result) in results.iter().enumerate() {
        output.push_str(&format!(
            "{}. {} [{}] (相关度: {:.0}%)\n",
            i + 1, result.entity.name, result.entity.entity_type, result.relevance * 100.0,
        ));
        if let Some(ref desc) = result.entity.description {
            output.push_str(&format!("   描述: {}\n", desc));
        }
        if !result.relations.is_empty() {
            output.push_str("   关系:\n");
            for rel in &result.relations {
                output.push_str(&format!(
                    "     - {} {} {} (置信度: {:.0}%)\n",
                    rel.source_entity_id, rel.relation_type, rel.target_entity_id, rel.confidence * 100.0,
                ));
            }
        }
        if !result.source_snippet.is_empty() {
            output.push_str(&format!("   来源: {}\n", &result.source_snippet[..result.source_snippet.len().min(200)]));
        }
        output.push_str("\n");
    }
    Ok(output)
}
```

- [ ] **Step 3: Register tool in AgentToolRegistry**

Follow the pattern in `cloud/src/modules/agent/tools/service.rs` — add the tool registration. The exact registration depends on how existing tools are registered. Read `cloud/src/modules/agent/mod.rs` or the tool registry setup.

- [ ] **Step 4: Inject knowledge context into Agent system_prompt**

In `cloud/src/modules/agent/service.rs`, find where `build_system_prompt()` is called. Add:

```rust
// Before building system_prompt, inject knowledge context
let knowledge_ctx = state.knowledge_service.build_context(workspace_id).await;
let system_prompt = if knowledge_ctx.is_empty() {
    base_system_prompt.to_string()
} else {
    format!("{}\n\n{}", base_system_prompt, knowledge_ctx)
};
```

- [ ] **Step 5: Build and verify**

Run: `cargo build`
Expected: Tool compiles, Agent service integration works

- [ ] **Step 6: Commit**

```bash
git add cloud/src/modules/agent/tools/knowledge.rs cloud/src/modules/agent/service.rs
git commit -m "feat: add search_knowledge agent tool and context injection"
```

---

### Task 7: Frontend Knowledge API Client

**Files:**
- Create: `web/src/api/knowledge.ts`

- [ ] **Step 1: Write the API client following the workspace-resources.ts pattern**

```typescript
// web/src/api/knowledge.ts

import { apiGet, apiPost, apiPut, apiDelete, apiUpload } from './client.js';

export interface KnowledgeDocument {
  id: string;
  workspace_id: string;
  title: string;
  content: string;
  tags: string[];
  parse_status: 'pending' | 'parsing' | 'parsed' | 'failed';
  created_at: string;
  updated_at: string;
}

export interface KnowledgeEntity {
  id: string;
  workspace_id: string;
  source_document_id: string;
  entity_type: string;
  name: string;
  description: string | null;
  properties: Record<string, unknown>;
  tags: string[];
  file_ids: string[];
  device_id: string | null;
  confidence: number;
  created_at: string;
  updated_at: string;
}

export interface KnowledgeRelation {
  id: string;
  workspace_id: string;
  source_entity_id: string;
  target_entity_id: string;
  relation_type: string;
  properties: Record<string, unknown>;
  confidence: number;
}

export interface KnowledgeParseJob {
  id: string;
  document_id: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  error_message: string | null;
  result_summary: ParseResultSummary | null;
  created_at: string;
  updated_at: string;
}

export interface ParseResultSummary {
  entity_count: number;
  relation_count: number;
  diff: ParseDiff | null;
}

export interface ParseDiff {
  added: number;
  removed: number;
  modified: number;
}

export interface CreateKnowledgeDocumentRequest {
  title: string;
  content: string;
  tags?: string[];
  file_ids?: string[];
}

export interface UpdateKnowledgeDocumentRequest {
  title?: string;
  content?: string;
  tags?: string[];
}

export interface PreviewParseRequest {
  content: string;
}

export interface PreviewParseResponse {
  entities: KnowledgeEntity[];
  relations: KnowledgeRelation[];
}

export interface KnowledgeSearchResult {
  entity: KnowledgeEntity;
  relations: KnowledgeRelation[];
  source_snippet: string;
  relevance: number;
}

export interface DocumentListResponse {
  data: KnowledgeDocument[];
  total: number;
  page: number;
  page_size: number;
}

function getWorkspaceId(): string | null {
  if (typeof window === 'undefined') return null;
  return localStorage.getItem('workspace-id') || sessionStorage.getItem('workspace-id');
}

export const knowledgeApi = {
  // Documents
  async listDocuments(params?: {
    q?: string;
    tags?: string;
    status?: string;
    page?: number;
    page_size?: number;
  }) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<DocumentListResponse>(
      `/workspaces/${wsId}/knowledge/documents`,
      params as Record<string, unknown>,
    );
  },

  async createDocument(data: CreateKnowledgeDocumentRequest) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiPost<KnowledgeDocument>(
      `/workspaces/${wsId}/knowledge/documents`,
      data,
    );
  },

  async getDocument(documentId: string) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<KnowledgeDocument>(
      `/workspaces/${wsId}/knowledge/documents/${documentId}`,
    );
  },

  async updateDocument(documentId: string, data: UpdateKnowledgeDocumentRequest) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiPut<KnowledgeDocument>(
      `/workspaces/${wsId}/knowledge/documents/${documentId}`,
      data,
    );
  },

  async deleteDocument(documentId: string) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiDelete<void>(
      `/workspaces/${wsId}/knowledge/documents/${documentId}`,
    );
  },

  // Parse
  async triggerParse(documentId: string) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiPost<{ parse_id: string }>(
      `/workspaces/${wsId}/knowledge/documents/${documentId}/parse`,
    );
  },

  async previewParse(documentId: string, data: PreviewParseRequest) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiPost<PreviewParseResponse>(
      `/workspaces/${wsId}/knowledge/documents/${documentId}/preview`,
      data,
    );
  },

  async getParseJob(jobId: string) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<KnowledgeParseJob>(
      `/workspaces/${wsId}/knowledge/parse/${jobId}`,
    );
  },

  // Entities & Relations
  async listEntities(params?: { entity_type?: string; tags?: string }) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<KnowledgeEntity[]>(
      `/workspaces/${wsId}/knowledge/entities`,
      params as Record<string, unknown>,
    );
  },

  async updateEntity(entityId: string, data: Record<string, unknown>) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiPut<KnowledgeEntity>(
      `/workspaces/${wsId}/knowledge/entities/${entityId}`,
      data,
    );
  },

  async listRelations() {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<KnowledgeRelation[]>(
      `/workspaces/${wsId}/knowledge/relations`,
    );
  },

  // Search
  async searchKnowledge(params: {
    q: string;
    entity_type?: string;
    tags?: string;
    limit?: number;
  }) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<KnowledgeSearchResult[]>(
      `/workspaces/${wsId}/knowledge/search`,
      params as Record<string, unknown>,
    );
  },

  // Context (for debugging)
  async getContext() {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<string>(`/workspaces/${wsId}/knowledge/context`);
  },

  // File upload (reuse existing upload mechanism)
  async uploadFile(file: File, onProgress?: (pct: number) => void) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    const formData = new FormData();
    formData.append('file', file);
    return apiUpload<{ filePath: string; fileSize: number }>(
      `/workspaces/${wsId}/resources/upload`,
      formData,
      onProgress,
    );
  },
};
```

- [ ] **Step 2: Commit**

```bash
git add web/src/api/knowledge.ts
git commit -m "feat: add knowledge API client"
```

---

### Task 8: Frontend Knowledge Document List View

**Files:**
- Create: `web/src/ui/views/knowledge.ts`
- Create: `web/src/styles/views/knowledge.css`

- [ ] **Step 1: Read existing view and CSS patterns**

Read `web/src/ui/views/workspace-resources.ts` (first 100 lines) for Lit 3 component pattern.
Read `web/src/styles/views/workspace-resources.css` (first 80 lines) for CSS patterns.

- [ ] **Step 2: Write knowledge.css**

```css
/* web/src/styles/views/knowledge.css */

/* ── Document List ── */
.knowledge-view {
  padding: 24px;
}

.knowledge-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 24px;
}

.knowledge-header h2 {
  margin: 0;
  font-size: 1.5rem;
  font-weight: 600;
  color: var(--text);
}

/* Filter bar — reuse existing pattern */
.knowledge-filter-bar {
  display: flex;
  gap: 12px;
  margin-bottom: 20px;
  flex-wrap: wrap;
  align-items: center;
}

.knowledge-filter-bar input[type="search"] {
  padding: 8px 14px;
  border: 1px solid var(--border, #2a2d35);
  border-radius: 8px;
  background: var(--card);
  color: var(--text);
  font-size: 0.875rem;
  min-width: 240px;
  outline: none;
  transition: border-color 0.2s;
}

.knowledge-filter-bar input[type="search"]:focus {
  border-color: var(--accent);
}

.knowledge-filter-chip {
  padding: 6px 14px;
  border-radius: 20px;
  border: 1px solid var(--border, #2a2d35);
  background: var(--card);
  color: var(--text-dim, #888);
  font-size: 0.8125rem;
  cursor: pointer;
  transition: all 0.2s;
}

.knowledge-filter-chip.active {
  background: linear-gradient(135deg, #00d4ff22, #0098FF22);
  border-color: var(--accent);
  color: var(--accent);
}

/* Document grid */
.knowledge-doc-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: 16px;
}

/* Document card */
.knowledge-doc-card {
  background: var(--card);
  border: 1px solid var(--border, #2a2d35);
  border-radius: 12px;
  padding: 20px;
  cursor: pointer;
  transition: all 0.2s;
  position: relative;
}

.knowledge-doc-card:hover {
  border-color: var(--accent);
  transform: translateY(-2px);
  box-shadow: 0 4px 20px rgba(0, 152, 255, 0.1);
}

.knowledge-doc-card-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: 12px;
}

.knowledge-doc-card-title {
  font-size: 1rem;
  font-weight: 600;
  color: var(--text);
  margin: 0;
  line-height: 1.4;
}

.knowledge-doc-card-status {
  font-size: 0.75rem;
  padding: 2px 8px;
  border-radius: 10px;
  white-space: nowrap;
}

.knowledge-status-parsed {
  background: rgba(34, 197, 94, 0.15);
  color: var(--ok);
}

.knowledge-status-pending {
  background: rgba(245, 158, 11, 0.15);
  color: var(--warn);
}

.knowledge-status-failed {
  background: rgba(239, 68, 68, 0.15);
  color: var(--danger);
}

.knowledge-status-parsing {
  background: rgba(0, 152, 255, 0.15);
  color: var(--accent);
}

.knowledge-doc-card-meta {
  display: flex;
  gap: 16px;
  font-size: 0.8125rem;
  color: var(--text-dim, #888);
  margin-bottom: 12px;
}

.knowledge-doc-card-tags {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

.knowledge-doc-card-tag {
  font-size: 0.75rem;
  padding: 2px 10px;
  border-radius: 12px;
  background: rgba(0, 152, 255, 0.1);
  color: var(--accent);
}

/* Empty state */
.knowledge-empty {
  text-align: center;
  padding: 60px 20px;
  color: var(--text-dim, #888);
}

.knowledge-empty-icon {
  font-size: 3rem;
  margin-bottom: 16px;
  opacity: 0.3;
}

.knowledge-empty h3 {
  margin: 0 0 8px;
  font-size: 1.125rem;
  color: var(--text);
}

.knowledge-empty p {
  margin: 0 0 20px;
  font-size: 0.875rem;
}

/* ── Document Editor Modal ── */
.knowledge-editor-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
}

.knowledge-editor {
  background: var(--bg);
  border: 1px solid var(--border, #2a2d35);
  border-radius: 16px;
  width: 90vw;
  max-width: 1100px;
  height: 85vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.knowledge-editor-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 24px;
  border-bottom: 1px solid var(--border, #2a2d35);
}

.knowledge-editor-title-input {
  flex: 1;
  background: transparent;
  border: none;
  color: var(--text);
  font-size: 1.25rem;
  font-weight: 600;
  outline: none;
  padding: 0;
}

.knowledge-editor-title-input::placeholder {
  color: var(--text-dim, #666);
}

.knowledge-editor-close {
  background: transparent;
  border: none;
  color: var(--text-dim, #888);
  font-size: 1.5rem;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 6px;
  transition: all 0.2s;
}

.knowledge-editor-close:hover {
  background: var(--card);
  color: var(--text);
}

.knowledge-editor-body {
  display: flex;
  flex: 1;
  overflow: hidden;
}

.knowledge-editor-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.knowledge-editor-textarea {
  flex: 1;
  padding: 20px 24px;
  background: transparent;
  border: none;
  color: var(--text);
  font-size: 0.9375rem;
  line-height: 1.8;
  resize: none;
  outline: none;
  font-family: 'Menlo', 'Monaco', 'Courier New', monospace;
}

.knowledge-editor-textarea::placeholder {
  color: var(--text-dim, #555);
}

/* Live Preview Panel */
.knowledge-editor-preview {
  width: 380px;
  border-left: 1px solid var(--border, #2a2d35);
  padding: 16px;
  overflow-y: auto;
  background: var(--card);
}

.knowledge-preview-header {
  font-size: 0.8125rem;
  font-weight: 600;
  color: var(--text-dim, #888);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-bottom: 12px;
}

.knowledge-preview-entity {
  padding: 10px 12px;
  border-radius: 8px;
  background: var(--bg);
  margin-bottom: 8px;
}

.knowledge-preview-entity-name {
  font-size: 0.875rem;
  font-weight: 600;
  color: var(--text);
  margin-bottom: 4px;
}

.knowledge-preview-entity-type {
  font-size: 0.75rem;
  color: var(--accent);
}

.knowledge-preview-entity-tags {
  display: flex;
  gap: 4px;
  margin-top: 6px;
  flex-wrap: wrap;
}

.knowledge-preview-tag {
  font-size: 0.6875rem;
  padding: 1px 6px;
  border-radius: 8px;
  background: rgba(0, 152, 255, 0.1);
  color: var(--accent);
}

.knowledge-preview-confidence {
  font-size: 0.6875rem;
  color: var(--text-dim, #666);
}

.knowledge-preview-relation {
  font-size: 0.8125rem;
  color: var(--text-dim, #888);
  padding: 4px 0;
  border-bottom: 1px solid var(--border, #1a1d25);
}

.knowledge-preview-relation:last-child {
  border-bottom: none;
}

/* Parse Results Panel */
.knowledge-editor-footer {
  border-top: 1px solid var(--border, #2a2d35);
  padding: 12px 24px;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.knowledge-editor-tags {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  flex: 1;
}

.knowledge-editor-tag {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 10px;
  border-radius: 14px;
  font-size: 0.8125rem;
  background: rgba(0, 152, 255, 0.1);
  color: var(--accent);
  border: 1px solid rgba(0, 152, 255, 0.2);
}

.knowledge-editor-tag-remove {
  cursor: pointer;
  font-size: 0.875rem;
  opacity: 0.6;
}

.knowledge-editor-tag-remove:hover {
  opacity: 1;
}

.knowledge-editor-actions {
  display: flex;
  gap: 8px;
  align-items: center;
}

.knowledge-parse-hint {
  font-size: 0.75rem;
  color: var(--text-dim, #666);
  margin-right: 12px;
}

/* Diff view */
.knowledge-diff-added {
  background: rgba(34, 197, 94, 0.1);
  border-left: 3px solid var(--ok);
}

.knowledge-diff-removed {
  background: rgba(239, 68, 68, 0.1);
  border-left: 3px solid var(--danger);
  opacity: 0.5;
}

.knowledge-diff-modified {
  background: rgba(245, 158, 11, 0.1);
  border-left: 3px solid var(--warn);
}

/* Buttons (reuse existing .btn pattern) */
.knowledge-btn {
  padding: 8px 16px;
  border-radius: 8px;
  border: none;
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s;
}

.knowledge-btn-primary {
  background: linear-gradient(135deg, #00d4ff, #0098FF);
  color: #fff;
}

.knowledge-btn-primary:hover {
  opacity: 0.9;
  transform: translateY(-1px);
}

.knowledge-btn-secondary {
  background: var(--card);
  color: var(--text);
  border: 1px solid var(--border, #2a2d35);
}

.knowledge-btn-secondary:hover {
  border-color: var(--accent);
}

.knowledge-btn-ghost {
  background: transparent;
  color: var(--text-dim, #888);
}

.knowledge-btn-ghost:hover {
  background: var(--card);
  color: var(--text);
}

/* Spinner for parsing state */
.knowledge-spinner {
  display: inline-block;
  width: 14px;
  height: 14px;
  border: 2px solid var(--border, #2a2d35);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: knowledge-spin 0.6s linear infinite;
}

@keyframes knowledge-spin {
  to { transform: rotate(360deg); }
}

/* Card enter animation */
@keyframes knowledge-card-enter {
  from {
    opacity: 0;
    transform: translateY(12px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.knowledge-doc-card {
  animation: knowledge-card-enter 0.3s ease both;
}

.knowledge-doc-card:nth-child(1) { animation-delay: 0s; }
.knowledge-doc-card:nth-child(2) { animation-delay: 0.05s; }
.knowledge-doc-card:nth-child(3) { animation-delay: 0.1s; }
.knowledge-doc-card:nth-child(4) { animation-delay: 0.15s; }
.knowledge-doc-card:nth-child(5) { animation-delay: 0.2s; }
.knowledge-doc-card:nth-child(n+6) { animation-delay: 0.25s; }
```

- [ ] **Step 3: Write the Lit 3 knowledge view component**

```typescript
// web/src/ui/views/knowledge.ts

import { LitElement, html, css } from 'lit';
import { customElement, state, property } from 'lit/decorators.js';
import { knowledgeApi, type KnowledgeDocument } from '../../api/knowledge.js';

@customElement('knowledge-view')
export class KnowledgeView extends LitElement {
  static styles = css`
    /* Styles imported via knowledge.css — see Step 2 */
  `;

  @property({ type: String }) workspaceId: string = '';

  @state() private documents: KnowledgeDocument[] = [];
  @state() private total: number = 0;
  @state() private loading: boolean = true;
  @state() private searchQuery: string = '';
  @state() private statusFilter: string = '';
  @state() private tagFilter: string = '';

  // Editor state
  @state() private editorOpen: boolean = false;
  @state() private editingDoc: KnowledgeDocument | null = null;
  @state() private editorTitle: string = '';
  @state() private editorContent: string = '';
  @state() private editorTags: string[] = [];
  @state() private editorSaving: boolean = false;

  async connectedCallback() {
    super.connectedCallback();
    await this.loadDocuments();
  }

  async loadDocuments() {
    this.loading = true;
    try {
      const res = await knowledgeApi.listDocuments({
        q: this.searchQuery || undefined,
        status: this.statusFilter || undefined,
        tags: this.tagFilter || undefined,
      });
      this.documents = res.data;
      this.total = res.total;
    } catch (e) {
      console.error('Failed to load documents:', e);
    } finally {
      this.loading = false;
    }
  }

  private async handleSearch(e: Event) {
    this.searchQuery = (e.target as HTMLInputElement).value;
    await this.loadDocuments();
  }

  private async handleStatusFilter(status: string) {
    this.statusFilter = this.statusFilter === status ? '' : status;
    await this.loadDocuments();
  }

  private openNewDocument() {
    this.editingDoc = null;
    this.editorTitle = '';
    this.editorContent = '';
    this.editorTags = [];
    this.editorOpen = true;
  }

  private openDocument(doc: KnowledgeDocument) {
    this.editingDoc = doc;
    this.editorTitle = doc.title;
    this.editorContent = doc.content;
    this.editorTags = [...doc.tags];
    this.editorOpen = true;
  }

  private closeEditor() {
    this.editorOpen = false;
    this.editingDoc = null;
  }

  private async handleSave() {
    this.editorSaving = true;
    try {
      if (this.editingDoc) {
        await knowledgeApi.updateDocument(this.editingDoc.id, {
          title: this.editorTitle,
          content: this.editorContent,
          tags: this.editorTags,
        });
      } else {
        await knowledgeApi.createDocument({
          title: this.editorTitle,
          content: this.editorContent,
          tags: this.editorTags,
        });
      }
      this.closeEditor();
      await this.loadDocuments();
    } catch (e) {
      console.error('Failed to save document:', e);
    } finally {
      this.editorSaving = false;
    }
  }

  private async handleDelete(doc: KnowledgeDocument) {
    if (!confirm(`确定要删除「${doc.title}」吗？此操作将同时删除关联的实体和关系。`)) return;
    try {
      await knowledgeApi.deleteDocument(doc.id);
      await this.loadDocuments();
    } catch (e) {
      console.error('Failed to delete document:', e);
    }
  }

  private addTag(tag: string) {
    const trimmed = tag.trim();
    if (trimmed && !this.editorTags.includes(trimmed)) {
      this.editorTags = [...this.editorTags, trimmed];
    }
  }

  private removeTag(tag: string) {
    this.editorTags = this.editorTags.filter(t => t !== tag);
  }

  render() {
    return html`
      <div class="knowledge-view">
        ${this.renderHeader()}
        ${this.renderFilters()}
        ${this.loading
          ? html`<div class="knowledge-empty"><p>加载中...</p></div>`
          : this.renderDocumentGrid()}
        ${this.editorOpen ? this.renderEditor() : ''}
      </div>
    `;
  }

  private renderHeader() {
    return html`
      <div class="knowledge-header">
        <h2>知识文档</h2>
        <button class="knowledge-btn knowledge-btn-primary" @click=${this.openNewDocument}>
          + 新建文档
        </button>
      </div>
    `;
  }

  private renderFilters() {
    return html`
      <div class="knowledge-filter-bar">
        <input
          type="search"
          placeholder="搜索文档..."
          .value=${this.searchQuery}
          @input=${this.handleSearch}
        />
        <button
          class="knowledge-filter-chip ${this.statusFilter === 'parsed' ? 'active' : ''}"
          @click=${() => this.handleStatusFilter('parsed')}
        >已解析</button>
        <button
          class="knowledge-filter-chip ${this.statusFilter === 'pending' ? 'active' : ''}"
          @click=${() => this.handleStatusFilter('pending')}
        >待解析</button>
        <button
          class="knowledge-filter-chip ${this.statusFilter === 'failed' ? 'active' : ''}"
          @click=${() => this.handleStatusFilter('failed')}
        >解析失败</button>
      </div>
    `;
  }

  private renderDocumentGrid() {
    if (this.documents.length === 0) {
      return html`
        <div class="knowledge-empty">
          <div class="knowledge-empty-icon">...</div>
          <h3>还没有知识文档</h3>
          <p>创建你的第一篇知识文档，描述工作区的空间、设备和功能要素</p>
          <button class="knowledge-btn knowledge-btn-primary" @click=${this.openNewDocument}>
            创建第一篇文档
          </button>
        </div>
      `;
    }

    return html`
      <div class="knowledge-doc-grid">
        ${this.documents.map(doc => this.renderDocumentCard(doc))}
      </div>
    `;
  }

  private renderDocumentCard(doc: KnowledgeDocument) {
    const statusClass = `knowledge-status-${doc.parse_status}`;
    const statusLabel: Record<string, string> = {
      pending: '待解析',
      parsing: '解析中...',
      parsed: '已解析',
      failed: '解析失败',
    };

    return html`
      <div class="knowledge-doc-card" @click=${() => this.openDocument(doc)}>
        <div class="knowledge-doc-card-header">
          <h3 class="knowledge-doc-card-title">${doc.title}</h3>
          <span class="knowledge-doc-card-status ${statusClass}">
            ${doc.parse_status === 'parsing'
              ? html`<span class="knowledge-spinner"></span> ${statusLabel[doc.parse_status]}`
              : statusLabel[doc.parse_status]}
          </span>
        </div>
        <div class="knowledge-doc-card-meta">
          <span>更新于 ${new Date(doc.updated_at).toLocaleDateString('zh-CN')}</span>
        </div>
        ${doc.tags.length > 0 ? html`
          <div class="knowledge-doc-card-tags">
            ${doc.tags.map(tag => html`<span class="knowledge-doc-card-tag">#${tag}</span>`)}
          </div>
        ` : ''}
        <div style="display:flex;justify-content:flex-end;margin-top:8px;">
          <button
            class="knowledge-btn knowledge-btn-ghost"
            @click=${(e: Event) => { e.stopPropagation(); this.handleDelete(doc); }}
            style="font-size:0.75rem;padding:4px 8px;"
          >删除</button>
        </div>
      </div>
    `;
  }

  private renderEditor() {
    return html`
      <div class="knowledge-editor-overlay" @click.self=${this.closeEditor}>
        <div class="knowledge-editor">
          <div class="knowledge-editor-header">
            <input
              class="knowledge-editor-title-input"
              placeholder="文档标题"
              .value=${this.editorTitle}
              @input=${(e: Event) => this.editorTitle = (e.target as HTMLInputElement).value}
            />
            <button class="knowledge-editor-close" @click=${this.closeEditor}>&times;</button>
          </div>
          <div class="knowledge-editor-body">
            <div class="knowledge-editor-main">
              <textarea
                class="knowledge-editor-textarea"
                placeholder="用 Markdown 描述你的工作区...&#10;&#10;例如：&#10;## 园区概况&#10;阳光科技园区，占地 5 万㎡，包含 3 栋建筑。&#10;&#10;## 设备清单&#10;- 机房 A：TH-A-01 温湿度传感器"
                .value=${this.editorContent}
                @input=${(e: Event) => this.editorContent = (e.target as HTMLTextAreaElement).value}
              ></textarea>
            </div>
            <!-- Live preview panel placeholder — populated via preview parse polling -->
            <div class="knowledge-editor-preview" id="preview-panel">
              <div class="knowledge-preview-header">实时预览</div>
              <p style="font-size:0.8125rem;color:var(--text-dim);text-align:center;margin-top:40px;">
                输入内容后将自动预览实体和关系
              </p>
            </div>
          </div>
          <div class="knowledge-editor-footer">
            <div class="knowledge-editor-tags">
              ${this.editorTags.map(tag => html`
                <span class="knowledge-editor-tag">
                  #${tag}
                  <span class="knowledge-editor-tag-remove" @click=${() => this.removeTag(tag)}>&times;</span>
                </span>
              `)}
              <input
                placeholder="添加标签..."
                style="background:transparent;border:none;color:var(--text-dim);font-size:0.8125rem;outline:none;width:100px;"
                @keydown=${(e: KeyboardEvent) => {
                  if (e.key === 'Enter') {
                    this.addTag((e.target as HTMLInputElement).value);
                    (e.target as HTMLInputElement).value = '';
                  }
                }}
              />
            </div>
            <div class="knowledge-editor-actions">
              <span class="knowledge-parse-hint">Ctrl+Enter 提交</span>
              <button class="knowledge-btn knowledge-btn-secondary" @click=${this.closeEditor}>取消</button>
              <button
                class="knowledge-btn knowledge-btn-primary"
                @click=${this.handleSave}
                ?disabled=${this.editorSaving}
              >${this.editingDoc ? '保存并重新解析' : '保存并解析'}</button>
            </div>
          </div>
        </div>
      </div>
    `;
  }
}
```

- [ ] **Step 4: Import CSS in the view or global styles**

Ensure `knowledge.css` is imported in the main stylesheet or directly in the component.

- [ ] **Step 5: Commit**

```bash
git add web/src/ui/views/knowledge.ts web/src/styles/views/knowledge.css
git commit -m "feat: add knowledge document list view and editor"
```

---

### Task 9: Integration — Wire Up Navigation and Final Build

**Files:**
- Modify: `web/src/ui/views/chat.ts` (or navigation component) — add link to knowledge view
- Modify: `cloud/src/main.rs` or routing setup — ensure knowledge routes are mounted

- [ ] **Step 1: Find and update workspace navigation**

Search for where the workspace sidebar/tab navigation is defined and add a "知识图谱" tab pointing to the knowledge view.

Run: `grep -r "workspace-resources" web/src/ui/ --include="*.ts" -l`

Find the navigation component and add the knowledge view route.

- [ ] **Step 2: Verify backend routes are mounted**

In `cloud/src/modules/workspace/handler.rs`, verify that knowledge routes are added to the main router. Check that `AppState` includes `knowledge_service`.

Run: `cargo build`
Expected: All routes compile and link correctly

- [ ] **Step 3: End-to-end smoke test**

Run: `cargo test`
Expected: All existing tests pass, no regressions

- [ ] **Step 4: Commit**

```bash
git add .
git commit -m "feat: wire up knowledge graph navigation and routes"
```

---

### Task 10: Final Review and Cleanup

- [ ] **Step 1: Run full build and tests**

```bash
cargo build --release
cargo test
cargo clippy
```

Expected: Clean build, all tests pass, no clippy warnings

- [ ] **Step 2: Verify all spec requirements are covered**

Check against the design spec:
- [x] Database migration (4 tables + indexes)
- [x] KnowledgeDocument, KnowledgeEntity, KnowledgeRelation types
- [x] KnowledgeRepository trait + SQLite impl
- [x] KnowledgeService (CRUD, parse pipeline, context gen, search)
- [x] Async parse with `knowledge_parse_jobs` polling
- [x] Live preview parse (POST /preview)
- [x] Parse diff view (ParseDiff in result_summary)
- [x] Entity→device linking (device_id field + update)
- [x] LLM error handling (retry with exponential backoff, timeout)
- [x] search_knowledge agent tool
- [x] Agent context injection (build_context → system_prompt)
- [x] Frontend API client
- [x] Frontend document list view with filters
- [x] Frontend editor modal with preview panel

- [ ] **Step 3: Commit final review**

```bash
git add .
git commit -m "chore: final review and cleanup for knowledge graph implementation"
```
