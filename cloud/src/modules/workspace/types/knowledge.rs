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

// Query params
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
