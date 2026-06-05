use async_trait::async_trait;
use sqlx::{FromRow, QueryBuilder, Row};
use tinyiothub_core::error::{Error, Result};
use tinyiothub_storage::sqlite::Database;

use super::super::types::{
    KnowledgeEntity, KnowledgeParseJob, KnowledgeRelation, KnowledgeSearchResult,
    ParseResultSummary, ResourceType, WorkspaceResource,
};
use crate::shared::utils::sql_security::escape_like_pattern;

// --- Row types ---

/// Row type for the unified resources table (document subset).
#[derive(Debug, Clone, FromRow)]
pub(crate) struct ResourceRow {
    pub id: String,
    pub workspace_id: String,
    pub resource_type: String,
    pub name: String,
    pub description: Option<String>,
    pub content: Option<String>,
    pub file_path: String,
    pub file_size: Option<i64>,
    pub tags: String,
    pub metadata: Option<String>,
    pub parse_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ResourceRow> for WorkspaceResource {
    fn from(row: ResourceRow) -> Self {
        let tags: Vec<String> = serde_json::from_str(&row.tags).unwrap_or_default();
        Self {
            id: row.id,
            workspace_id: row.workspace_id,
            resource_type: ResourceType::from_string(&row.resource_type)
                .unwrap_or(ResourceType::Document),
            name: row.name,
            description: row.description,
            content: row.content,
            file_path: row.file_path,
            file_size: row.file_size,
            tags,
            metadata: row.metadata,
            parse_status: row.parse_status,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub(crate) struct KnowledgeEntityRow {
    pub id: String,
    pub workspace_id: String,
    pub source_document_id: String,
    pub entity_type: String,
    pub name: String,
    pub description: Option<String>,
    pub properties: String, // JSON Value
    pub tags: String,       // JSON array string
    pub file_ids: String,   // JSON array string
    pub device_id: Option<String>,
    pub confidence: f64,
    pub created_at: String,
    pub updated_at: String,
}

impl From<KnowledgeEntityRow> for KnowledgeEntity {
    fn from(row: KnowledgeEntityRow) -> Self {
        let properties: serde_json::Value =
            serde_json::from_str(&row.properties).unwrap_or_default();
        let tags: Vec<String> = serde_json::from_str(&row.tags).unwrap_or_default();
        let file_ids: Vec<String> = serde_json::from_str(&row.file_ids).unwrap_or_default();
        Self {
            id: row.id,
            workspace_id: row.workspace_id,
            source_document_id: row.source_document_id,
            entity_type: row.entity_type,
            name: row.name,
            description: row.description,
            properties,
            tags,
            file_ids,
            device_id: row.device_id,
            confidence: row.confidence as f32,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub(crate) struct KnowledgeRelationRow {
    pub id: String,
    pub workspace_id: String,
    pub source_entity_id: String,
    pub target_entity_id: String,
    pub relation_type: String,
    pub properties: String, // JSON Value
    pub confidence: f64,
}

impl From<KnowledgeRelationRow> for KnowledgeRelation {
    fn from(row: KnowledgeRelationRow) -> Self {
        let properties: serde_json::Value =
            serde_json::from_str(&row.properties).unwrap_or_default();
        Self {
            id: row.id,
            workspace_id: row.workspace_id,
            source_entity_id: row.source_entity_id,
            target_entity_id: row.target_entity_id,
            relation_type: row.relation_type,
            properties,
            confidence: row.confidence as f32,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub(crate) struct KnowledgeParseJobRow {
    pub id: String,
    pub document_id: String,
    pub status: String,
    pub error_message: Option<String>,
    pub result_summary: Option<String>, // JSON string
    pub created_at: String,
    pub updated_at: String,
}

impl From<KnowledgeParseJobRow> for KnowledgeParseJob {
    fn from(row: KnowledgeParseJobRow) -> Self {
        let result_summary: Option<ParseResultSummary> =
            row.result_summary.and_then(|s| serde_json::from_str(&s).ok());
        Self {
            id: row.id,
            document_id: row.document_id,
            status: row.status,
            error_message: row.error_message,
            result_summary,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

// --- Trait ---

#[async_trait]
pub trait KnowledgeRepository: Send + Sync {
    // Documents (stored in unified resources table with resource_type='document')
    async fn list_documents(
        &self,
        workspace_id: &str,
        q: Option<&str>,
        tags: Option<&str>,
        status: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<(Vec<WorkspaceResource>, i64)>;
    async fn get_document(&self, id: &str) -> Result<Option<WorkspaceResource>>;
    async fn create_document(&self, doc: &WorkspaceResource) -> Result<()>;
    async fn update_document(
        &self,
        id: &str,
        name: Option<&str>,
        content: Option<&str>,
        file_path: Option<&str>,
        tags: Option<&str>,
        parse_status: Option<&str>,
    ) -> Result<Option<WorkspaceResource>>;
    async fn delete_document(&self, id: &str) -> Result<()>;

    // Entities
    async fn list_entities(
        &self,
        workspace_id: &str,
        entity_type: Option<&str>,
        tags: Option<&str>,
        document_id: Option<&str>,
    ) -> Result<Vec<KnowledgeEntity>>;
    async fn get_entity(&self, id: &str) -> Result<Option<KnowledgeEntity>>;
    async fn get_entities_by_document(&self, document_id: &str) -> Result<Vec<KnowledgeEntity>>;
    async fn upsert_entities(&self, entities: &[KnowledgeEntity]) -> Result<()>;
    async fn delete_entities_by_document(&self, document_id: &str) -> Result<()>;
    async fn update_entity(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        entity_type: Option<&str>,
        properties: Option<&str>,
        tags: Option<&str>,
        device_id: Option<Option<&str>>,
    ) -> Result<Option<KnowledgeEntity>>;

    // Relations
    async fn list_relations(&self, workspace_id: &str) -> Result<Vec<KnowledgeRelation>>;
    async fn get_relations_by_document(&self, document_id: &str) -> Result<Vec<KnowledgeRelation>>;
    async fn upsert_relations(&self, relations: &[KnowledgeRelation]) -> Result<()>;
    async fn delete_relations_by_document(&self, document_id: &str) -> Result<()>;

    // Search
    async fn search_knowledge(
        &self,
        workspace_id: &str,
        query: &str,
        entity_type: Option<&str>,
        tags: Option<&str>,
        limit: i64,
    ) -> Result<Vec<KnowledgeSearchResult>>;

    // Context
    async fn get_context_data(
        &self,
        workspace_id: &str,
    ) -> Result<(Vec<KnowledgeEntity>, Vec<KnowledgeRelation>)>;

    // Parse jobs
    async fn create_parse_job(&self, job: &KnowledgeParseJob) -> Result<()>;
    async fn get_parse_job(&self, id: &str) -> Result<Option<KnowledgeParseJob>>;
    async fn update_parse_job(
        &self,
        id: &str,
        status: &str,
        error_message: Option<&str>,
        result_summary: Option<&str>,
    ) -> Result<()>;
}

// --- SQLite implementation ---

#[derive(Debug, Clone)]
pub struct SqliteKnowledgeRepository {
    database: Database,
}

impl SqliteKnowledgeRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

/// Helper: split comma-separated tags string into trimmed non-empty Vec
fn split_tags(tags: Option<&str>) -> Vec<&str> {
    tags.map(|t| t.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default()
}

/// Helper: append optional filters to a QueryBuilder for document queries
fn append_document_filters(
    builder: &mut QueryBuilder<sqlx::Sqlite>,
    workspace_id: &str,
    q: Option<&str>,
    tags: Option<&str>,
    status: Option<&str>,
) {
    builder.push("workspace_id = ").push_bind(workspace_id);
    builder.push(" AND resource_type = ").push_bind(ResourceType::Document.as_str());

    if let Some(q) = q {
        let q_escaped = format!("%{}%", escape_like_pattern(q));
        builder.push(" AND (name LIKE ");
        builder.push_bind(&q_escaped);
        builder.push(" ESCAPE '\\' OR content LIKE ");
        builder.push_bind(&q_escaped);
        builder.push(" ESCAPE '\\'))");
    }

    let tag_list = split_tags(tags);
    for tag in &tag_list {
        builder.push(" AND EXISTS (SELECT 1 FROM json_each(tags) WHERE value = ");
        builder.push_bind(*tag);
        builder.push(")");
    }

    if let Some(status_filter) = status {
        builder.push(" AND parse_status = ");
        builder.push_bind(status_filter);
    }
}

#[async_trait]
impl KnowledgeRepository for SqliteKnowledgeRepository {
    // ── Documents ──

    async fn list_documents(
        &self,
        workspace_id: &str,
        q: Option<&str>,
        tags: Option<&str>,
        status: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<(Vec<WorkspaceResource>, i64)> {
        let page = page.max(1);
        let page_size = page_size.clamp(1, 100);
        let offset = (page - 1) * page_size;

        // Count query
        let mut count_builder = QueryBuilder::new("SELECT COUNT(*) as cnt FROM resources WHERE ");
        append_document_filters(&mut count_builder, workspace_id, q, tags, status);
        let count_row: (i64,) =
            count_builder.build_query_as().fetch_one(self.database.pool()).await?;
        let total = count_row.0;

        // Data query
        let mut data_builder = QueryBuilder::new("SELECT * FROM resources WHERE ");
        append_document_filters(&mut data_builder, workspace_id, q, tags, status);
        data_builder.push(" ORDER BY created_at DESC LIMIT ");
        data_builder.push_bind(page_size);
        data_builder.push(" OFFSET ");
        data_builder.push_bind(offset);

        let rows =
            data_builder.build_query_as::<ResourceRow>().fetch_all(self.database.pool()).await?;

        let docs: Vec<WorkspaceResource> = rows.into_iter().map(Into::into).collect();
        Ok((docs, total))
    }

    async fn get_document(&self, id: &str) -> Result<Option<WorkspaceResource>> {
        let row = sqlx::query_as::<_, ResourceRow>("SELECT * FROM resources WHERE id = ?")
            .bind(id)
            .fetch_optional(self.database.pool())
            .await?;

        Ok(row.map(Into::into))
    }

    async fn create_document(&self, doc: &WorkspaceResource) -> Result<()> {
        let tags_json = serde_json::to_string(&doc.tags).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            "INSERT INTO resources (id, workspace_id, resource_type, name, content, tags, parse_status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&doc.id)
        .bind(&doc.workspace_id)
        .bind(doc.resource_type.as_str())
        .bind(&doc.name)
        .bind(&doc.content)
        .bind(&tags_json)
        .bind(&doc.parse_status)
        .bind(&doc.created_at)
        .bind(&doc.updated_at)
        .execute(self.database.pool())
        .await?;

        Ok(())
    }

    async fn update_document(
        &self,
        id: &str,
        name: Option<&str>,
        content: Option<&str>,
        file_path: Option<&str>,
        tags: Option<&str>,
        parse_status: Option<&str>,
    ) -> Result<Option<WorkspaceResource>> {
        let mut builder = QueryBuilder::new("UPDATE resources SET ");
        let mut has_updates = false;
        let now = chrono::Utc::now().to_rfc3339();

        if let Some(t) = name {
            if has_updates {
                builder.push(", ");
            }
            builder.push("name = ").push_bind(t);
            has_updates = true;
        }

        if let Some(c) = content {
            if has_updates {
                builder.push(", ");
            }
            builder.push("content = ").push_bind(c);
            has_updates = true;
        }

        if let Some(fp) = file_path {
            if has_updates {
                builder.push(", ");
            }
            builder.push("file_path = ").push_bind(fp);
            has_updates = true;
        }

        if let Some(tag_str) = tags {
            if has_updates {
                builder.push(", ");
            }
            let tags_list: Vec<&str> =
                tag_str.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
            let tags_json = serde_json::to_string(&tags_list).unwrap_or_else(|_| "[]".to_string());
            builder.push("tags = ").push_bind(tags_json);
            has_updates = true;
        }

        if let Some(ps) = parse_status {
            if has_updates {
                builder.push(", ");
            }
            builder.push("parse_status = ").push_bind(ps);
            has_updates = true;
        }

        if !has_updates {
            return self.get_document(id).await;
        }

        builder.push(", updated_at = ").push_bind(&now);
        builder.push(" WHERE id = ").push_bind(id);

        let result = builder.build().execute(self.database.pool()).await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }

        self.get_document(id).await
    }

    async fn delete_document(&self, id: &str) -> Result<()> {
        // Delete relations referencing entities of this document
        sqlx::query(
            "DELETE FROM knowledge_relations WHERE source_entity_id IN (SELECT id FROM knowledge_entities WHERE source_document_id = ?) OR target_entity_id IN (SELECT id FROM knowledge_entities WHERE source_document_id = ?)",
        )
        .bind(id)
        .bind(id)
        .execute(self.database.pool())
        .await?;

        // Delete entities belonging to this document
        sqlx::query("DELETE FROM knowledge_entities WHERE source_document_id = ?")
            .bind(id)
            .execute(self.database.pool())
            .await?;

        // Delete parse jobs for this document
        sqlx::query("DELETE FROM knowledge_parse_jobs WHERE document_id = ?")
            .bind(id)
            .execute(self.database.pool())
            .await?;

        // Delete the document itself
        sqlx::query("DELETE FROM resources WHERE id = ?")
            .bind(id)
            .execute(self.database.pool())
            .await?;

        Ok(())
    }

    // ── Entities ──

    async fn list_entities(
        &self,
        workspace_id: &str,
        entity_type: Option<&str>,
        tags: Option<&str>,
        document_id: Option<&str>,
    ) -> Result<Vec<KnowledgeEntity>> {
        let mut builder =
            QueryBuilder::new("SELECT * FROM knowledge_entities WHERE workspace_id = ");
        builder.push_bind(workspace_id);

        if let Some(et) = entity_type {
            builder.push(" AND entity_type = ");
            builder.push_bind(et);
        }

        if let Some(did) = document_id {
            builder.push(" AND source_document_id = ");
            builder.push_bind(did);
        }

        let tag_list = split_tags(tags);
        for tag in &tag_list {
            builder.push(" AND EXISTS (SELECT 1 FROM json_each(tags) WHERE value = ");
            builder.push_bind(*tag);
            builder.push(")");
        }

        builder.push(" ORDER BY created_at DESC");

        let rows =
            builder.build_query_as::<KnowledgeEntityRow>().fetch_all(self.database.pool()).await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_entity(&self, id: &str) -> Result<Option<KnowledgeEntity>> {
        let row = sqlx::query_as::<_, KnowledgeEntityRow>(
            "SELECT * FROM knowledge_entities WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(Into::into))
    }

    async fn get_entities_by_document(&self, document_id: &str) -> Result<Vec<KnowledgeEntity>> {
        let rows = sqlx::query_as::<_, KnowledgeEntityRow>(
            "SELECT * FROM knowledge_entities WHERE source_document_id = ? ORDER BY created_at DESC",
        )
        .bind(document_id)
        .fetch_all(self.database.pool())
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn upsert_entities(&self, entities: &[KnowledgeEntity]) -> Result<()> {
        for entity in entities {
            let properties_json =
                serde_json::to_string(&entity.properties).unwrap_or_else(|_| "{}".to_string());
            let tags_json =
                serde_json::to_string(&entity.tags).unwrap_or_else(|_| "[]".to_string());
            let file_ids_json =
                serde_json::to_string(&entity.file_ids).unwrap_or_else(|_| "[]".to_string());
            let now = chrono::Utc::now().to_rfc3339();

            sqlx::query(
                "INSERT OR REPLACE INTO knowledge_entities (id, workspace_id, source_document_id, entity_type, name, description, properties, tags, file_ids, device_id, confidence, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&entity.id)
            .bind(&entity.workspace_id)
            .bind(&entity.source_document_id)
            .bind(&entity.entity_type)
            .bind(&entity.name)
            .bind(&entity.description)
            .bind(&properties_json)
            .bind(&tags_json)
            .bind(&file_ids_json)
            .bind(&entity.device_id)
            .bind(entity.confidence as f64)
            .bind(&entity.created_at)
            .bind(&now)
            .execute(self.database.pool())
            .await?;
        }

        Ok(())
    }

    async fn delete_entities_by_document(&self, document_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM knowledge_entities WHERE source_document_id = ?")
            .bind(document_id)
            .execute(self.database.pool())
            .await?;

        Ok(())
    }

    async fn update_entity(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        entity_type: Option<&str>,
        properties: Option<&str>,
        tags: Option<&str>,
        device_id: Option<Option<&str>>,
    ) -> Result<Option<KnowledgeEntity>> {
        let mut builder = QueryBuilder::new("UPDATE knowledge_entities SET ");
        let mut has_updates = false;
        let now = chrono::Utc::now().to_rfc3339();

        if let Some(n) = name {
            if has_updates {
                builder.push(", ");
            }
            builder.push("name = ").push_bind(n);
            has_updates = true;
        }

        if let Some(d) = description {
            if has_updates {
                builder.push(", ");
            }
            builder.push("description = ").push_bind(d);
            has_updates = true;
        }

        if let Some(et) = entity_type {
            if has_updates {
                builder.push(", ");
            }
            builder.push("entity_type = ").push_bind(et);
            has_updates = true;
        }

        if let Some(props_str) = properties {
            if has_updates {
                builder.push(", ");
            }
            builder.push("properties = ").push_bind(props_str);
            has_updates = true;
        }

        if let Some(tags_str) = tags {
            if has_updates {
                builder.push(", ");
            }
            let tags_list: Vec<&str> =
                tags_str.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
            let tags_json = serde_json::to_string(&tags_list).unwrap_or_else(|_| "[]".to_string());
            builder.push("tags = ").push_bind(tags_json);
            has_updates = true;
        }

        if let Some(device_id_val) = device_id {
            if has_updates {
                builder.push(", ");
            }
            builder.push("device_id = ");
            if let Some(did) = device_id_val {
                builder.push_bind(did);
            } else {
                builder.push("NULL");
            }
            has_updates = true;
        }

        if !has_updates {
            return self.get_entity(id).await;
        }

        builder.push(", updated_at = ").push_bind(&now);
        builder.push(" WHERE id = ").push_bind(id);

        let result = builder.build().execute(self.database.pool()).await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }

        self.get_entity(id).await
    }

    // ── Relations ──

    async fn list_relations(&self, workspace_id: &str) -> Result<Vec<KnowledgeRelation>> {
        let rows = sqlx::query_as::<_, KnowledgeRelationRow>(
            "SELECT * FROM knowledge_relations WHERE workspace_id = ?",
        )
        .bind(workspace_id)
        .fetch_all(self.database.pool())
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_relations_by_document(&self, document_id: &str) -> Result<Vec<KnowledgeRelation>> {
        let rows = sqlx::query_as::<_, KnowledgeRelationRow>(
            "SELECT DISTINCT r.* FROM knowledge_relations r INNER JOIN knowledge_entities e ON r.source_entity_id = e.id OR r.target_entity_id = e.id WHERE e.source_document_id = ?",
        )
        .bind(document_id)
        .fetch_all(self.database.pool())
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn upsert_relations(&self, relations: &[KnowledgeRelation]) -> Result<()> {
        for relation in relations {
            let properties_json =
                serde_json::to_string(&relation.properties).unwrap_or_else(|_| "{}".to_string());

            sqlx::query(
                "INSERT OR REPLACE INTO knowledge_relations (id, workspace_id, source_entity_id, target_entity_id, relation_type, properties, confidence) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&relation.id)
            .bind(&relation.workspace_id)
            .bind(&relation.source_entity_id)
            .bind(&relation.target_entity_id)
            .bind(&relation.relation_type)
            .bind(&properties_json)
            .bind(relation.confidence as f64)
            .execute(self.database.pool())
            .await?;
        }

        Ok(())
    }

    async fn delete_relations_by_document(&self, document_id: &str) -> Result<()> {
        sqlx::query(
            "DELETE FROM knowledge_relations WHERE source_entity_id IN (SELECT id FROM knowledge_entities WHERE source_document_id = ?) OR target_entity_id IN (SELECT id FROM knowledge_entities WHERE source_document_id = ?)",
        )
        .bind(document_id)
        .bind(document_id)
        .execute(self.database.pool())
        .await?;

        Ok(())
    }

    // ── Search ──

    async fn search_knowledge(
        &self,
        workspace_id: &str,
        query: &str,
        entity_type: Option<&str>,
        tags: Option<&str>,
        limit: i64,
    ) -> Result<Vec<KnowledgeSearchResult>> {
        let like_pattern = format!("%{}%", escape_like_pattern(query));

        // Build entity search with relevance scoring and source snippet
        let mut builder = QueryBuilder::new("SELECT e.*, (CASE WHEN e.name LIKE ");
        builder.push_bind(like_pattern.clone());
        builder.push(" ESCAPE '\\' THEN 3 ELSE 0 END) + (CASE WHEN e.description LIKE ");
        builder.push_bind(like_pattern.clone());
        builder.push(" ESCAPE '\\' THEN 2 ELSE 0 END) as relevance, COALESCE(substr(d.content, 1, 500), '') as source_snippet FROM knowledge_entities e LEFT JOIN resources d ON d.id = e.source_document_id WHERE e.workspace_id = ");
        builder.push_bind(workspace_id);

        builder.push(" AND (e.name LIKE ");
        builder.push_bind(like_pattern.clone());
        builder.push(" ESCAPE '\\' OR e.description LIKE ");
        builder.push_bind(like_pattern);
        builder.push(" ESCAPE '\\'))");

        if let Some(et) = entity_type {
            builder.push(" AND e.entity_type = ");
            builder.push_bind(et);
        }

        let tag_list = split_tags(tags);
        for tag in &tag_list {
            builder.push(" AND EXISTS (SELECT 1 FROM json_each(e.tags) WHERE value = ");
            builder.push_bind(*tag);
            builder.push(")");
        }

        builder.push(" ORDER BY relevance DESC LIMIT ");
        builder.push_bind(limit);

        let rows = builder
            .build()
            .fetch_all(self.database.pool())
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;

        // Parse results manually (entity fields + relevance + snippet)
        let mut entity_results: Vec<(KnowledgeEntity, f64, String)> = Vec::new();
        for row in &rows {
            let entity_row = KnowledgeEntityRow {
                id: row.get("id"),
                workspace_id: row.get("workspace_id"),
                source_document_id: row.get("source_document_id"),
                entity_type: row.get("entity_type"),
                name: row.get("name"),
                description: row.get("description"),
                properties: row.get("properties"),
                tags: row.get("tags"),
                file_ids: row.get("file_ids"),
                device_id: row.get("device_id"),
                confidence: row.get("confidence"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            };
            let relevance: f64 = row.get("relevance");
            let source_snippet: String = row.get("source_snippet");

            entity_results.push((KnowledgeEntity::from(entity_row), relevance, source_snippet));
        }

        if entity_results.is_empty() {
            return Ok(Vec::new());
        }

        // Fetch relations for all matched entities
        let entity_ids: Vec<&str> = entity_results.iter().map(|(e, _, _)| e.id.as_str()).collect();

        let mut rel_builder = QueryBuilder::new("SELECT * FROM knowledge_relations WHERE ");
        rel_builder.push("source_entity_id IN (");
        let mut first = true;
        for id in &entity_ids {
            if !first {
                rel_builder.push(", ");
            }
            first = false;
            rel_builder.push_bind(*id);
        }
        rel_builder.push(") OR target_entity_id IN (");
        first = true;
        for id in &entity_ids {
            if !first {
                rel_builder.push(", ");
            }
            first = false;
            rel_builder.push_bind(*id);
        }
        rel_builder.push(")");

        let rel_rows = rel_builder
            .build_query_as::<KnowledgeRelationRow>()
            .fetch_all(self.database.pool())
            .await?;

        let all_relations: Vec<KnowledgeRelation> = rel_rows.into_iter().map(Into::into).collect();

        // Build results with relations grouped by entity
        let mut results: Vec<KnowledgeSearchResult> = Vec::new();
        for (entity, relevance, source_snippet) in entity_results {
            let entity_relations: Vec<KnowledgeRelation> = all_relations
                .iter()
                .filter(|r| r.source_entity_id == entity.id || r.target_entity_id == entity.id)
                .cloned()
                .collect();

            results.push(KnowledgeSearchResult {
                entity,
                relations: entity_relations,
                source_snippet,
                relevance: relevance as f32,
            });
        }

        Ok(results)
    }

    // ── Context ──

    async fn get_context_data(
        &self,
        workspace_id: &str,
    ) -> Result<(Vec<KnowledgeEntity>, Vec<KnowledgeRelation>)> {
        let entities = sqlx::query_as::<_, KnowledgeEntityRow>(
            "SELECT * FROM knowledge_entities WHERE workspace_id = ?",
        )
        .bind(workspace_id)
        .fetch_all(self.database.pool())
        .await?;

        let relations = sqlx::query_as::<_, KnowledgeRelationRow>(
            "SELECT * FROM knowledge_relations WHERE workspace_id = ?",
        )
        .bind(workspace_id)
        .fetch_all(self.database.pool())
        .await?;

        Ok((
            entities.into_iter().map(Into::into).collect(),
            relations.into_iter().map(Into::into).collect(),
        ))
    }

    // ── Parse Jobs ──

    async fn create_parse_job(&self, job: &KnowledgeParseJob) -> Result<()> {
        let result_summary_json =
            job.result_summary.as_ref().and_then(|s| serde_json::to_string(s).ok());

        sqlx::query(
            "INSERT INTO knowledge_parse_jobs (id, document_id, status, error_message, result_summary, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&job.id)
        .bind(&job.document_id)
        .bind(&job.status)
        .bind(&job.error_message)
        .bind(&result_summary_json)
        .bind(&job.created_at)
        .bind(&job.updated_at)
        .execute(self.database.pool())
        .await?;

        Ok(())
    }

    async fn get_parse_job(&self, id: &str) -> Result<Option<KnowledgeParseJob>> {
        let row = sqlx::query_as::<_, KnowledgeParseJobRow>(
            "SELECT * FROM knowledge_parse_jobs WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(Into::into))
    }

    async fn update_parse_job(
        &self,
        id: &str,
        status: &str,
        error_message: Option<&str>,
        result_summary: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE knowledge_parse_jobs SET status = ?, error_message = ?, result_summary = ?, updated_at = ? WHERE id = ?",
        )
        .bind(status)
        .bind(error_message)
        .bind(result_summary)
        .bind(&now)
        .bind(id)
        .execute(self.database.pool())
        .await?;

        Ok(())
    }
}
