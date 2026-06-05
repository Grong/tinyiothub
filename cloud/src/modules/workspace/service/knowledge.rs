use std::{sync::Arc, time::Duration};

use tinyiothub_core::error::Error;

use super::super::{
    repo::KnowledgeRepository,
    types::{ResourceType, WorkspaceResource, extract_file_path_from_content, knowledge::*},
};
use crate::shared::error::Result;

/// Service for workspace knowledge graph operations.
///
/// Handles document CRUD, LLM-powered parse pipeline, context generation
/// for Agent system prompt injection, semantic search, and tag generation.
pub struct KnowledgeService {
    repo: Arc<dyn KnowledgeRepository>,
}

impl KnowledgeService {
    pub fn new(repo: Arc<dyn KnowledgeRepository>) -> Self {
        Self { repo }
    }

    // ── Document CRUD ──

    /// List documents with pagination and optional filters.
    pub async fn list_documents(
        &self,
        workspace_id: &str,
        q: Option<&str>,
        tags: Option<&str>,
        status: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<(Vec<WorkspaceResource>, i64)> {
        self.repo.list_documents(workspace_id, q, tags, status, page, page_size).await
    }

    /// Get a single document by ID.
    pub async fn get_document(&self, id: &str) -> Result<Option<WorkspaceResource>> {
        self.repo.get_document(id).await
    }

    /// Create a new document with auto-generated ID and "pending" parse status.
    pub async fn create_document(
        &self,
        workspace_id: &str,
        title: String,
        content: String,
        tags: Vec<String>,
    ) -> Result<WorkspaceResource> {
        let now = chrono::Utc::now().to_rfc3339();
        let id = format!("doc-{}", uuid::Uuid::new_v4());

        let file_path = extract_file_path_from_content(&content);

        let doc = WorkspaceResource {
            id,
            workspace_id: workspace_id.to_string(),
            resource_type: ResourceType::Document,
            name: title,
            description: None,
            content: Some(content),
            file_path,
            file_size: None,
            tags,
            metadata: None,
            parse_status: Some("pending".to_string()),
            created_at: now.clone(),
            updated_at: now,
        };

        self.repo.create_document(&doc).await?;
        Ok(doc)
    }

    /// Update an existing document. Sets parse_status to "pending" if content changed.
    pub async fn update_document(
        &self,
        id: &str,
        title: Option<String>,
        content: Option<String>,
        tags: Option<Vec<String>>,
    ) -> Result<Option<WorkspaceResource>> {
        // Determine if we need to reset parse status
        let mut parse_status: Option<&str> = None;

        if let Some(ref new_content) = content
            && let Some(existing) = self.repo.get_document(id).await?
            && existing.content.as_deref() != Some(new_content.as_str())
        {
            parse_status = Some("pending");
        }

        // Extract file_path from content when content is provided
        let file_path = content.as_ref().map(|c| extract_file_path_from_content(c));

        // Convert tags Vec to comma-separated string for the repo
        let tags_str: Option<String> = tags.map(|t| t.join(","));

        self.repo
            .update_document(
                id,
                title.as_deref(),
                content.as_deref(),
                file_path.as_deref(),
                tags_str.as_deref(),
                parse_status,
            )
            .await
    }

    /// Delete a document and its associated entities, relations, and parse jobs.
    pub async fn delete_document(&self, id: &str) -> Result<()> {
        self.repo.delete_document(id).await
    }

    // ── Entity Operations ──

    /// List entities for a workspace with optional filters.
    pub async fn list_entities(
        &self,
        workspace_id: &str,
        entity_type: Option<&str>,
        tags: Option<&str>,
        document_id: Option<&str>,
    ) -> Result<Vec<KnowledgeEntity>> {
        self.repo.list_entities(workspace_id, entity_type, tags, document_id).await
    }

    /// Get a single entity by ID.
    pub async fn get_entity(&self, id: &str) -> Result<Option<KnowledgeEntity>> {
        self.repo.get_entity(id).await
    }

    /// Update an entity with optional fields.
    pub async fn update_entity(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        entity_type: Option<&str>,
        properties: Option<&str>,
        tags: Option<&str>,
        device_id: Option<Option<&str>>,
    ) -> Result<Option<KnowledgeEntity>> {
        self.repo
            .update_entity(id, name, description, entity_type, properties, tags, device_id)
            .await
    }

    // ── Relation Operations ──

    /// List all relations for a workspace.
    pub async fn list_relations(&self, workspace_id: &str) -> Result<Vec<KnowledgeRelation>> {
        self.repo.list_relations(workspace_id).await
    }

    // ── Search ──

    /// Search knowledge graph entities by query text.
    pub async fn search_knowledge(
        &self,
        workspace_id: &str,
        query: &str,
        entity_type: Option<&str>,
        tags: Option<&str>,
        limit: i64,
    ) -> Result<Vec<KnowledgeSearchResult>> {
        self.repo.search_knowledge(workspace_id, query, entity_type, tags, limit).await
    }

    // ── Parse Pipeline ──

    /// Trigger an async parse job for a document. Returns the parse job ID immediately.
    ///
    /// The actual parsing runs in the background via `tokio::spawn`.
    pub async fn trigger_parse(&self, document_id: &str, workspace_id: &str) -> Result<String> {
        let now = chrono::Utc::now().to_rfc3339();
        let parse_id = format!("job-{}", uuid::Uuid::new_v4());

        // Validate document exists
        let doc = self.repo.get_document(document_id).await?.ok_or_else(|| {
            Error::InvalidArgument(format!("document not found: {}", document_id))
        })?;

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

        // Update document status to "parsing"
        self.repo.update_document(document_id, None, None, None, None, Some("parsing")).await?;

        // Spawn background task
        let repo = Arc::clone(&self.repo);
        let doc_id = document_id.to_string();
        let ws_id = workspace_id.to_string();
        let pid = parse_id.clone();
        let doc_content = doc.content.unwrap_or_default();

        tokio::spawn(async move {
            run_parse(repo, &doc_id, &ws_id, &pid, &doc_content).await;
        });

        Ok(parse_id)
    }

    /// Get a parse job by ID.
    pub async fn get_parse_job(&self, id: &str) -> Result<Option<KnowledgeParseJob>> {
        self.repo.get_parse_job(id).await
    }

    /// Preview parse without persisting to DB. Returns entities and relations.
    ///
    /// Uses a lighter prompt and shorter timeout (5s) for quick previews.
    pub async fn preview_parse(
        &self,
        content: &str,
        workspace_id: &str,
    ) -> Result<PreviewParseResponse> {
        let (entities, relations) = call_llm_preview(content, workspace_id).await?;
        Ok(PreviewParseResponse { entities, relations })
    }

    // ── Context Generation ──

    /// Build a tree-structured text representation of the workspace knowledge graph.
    ///
    /// This is intended for injection into the Agent's system prompt to provide
    /// persistent world knowledge.
    pub async fn build_context(&self, workspace_id: &str) -> Result<String> {
        let (entities, relations) = self.repo.get_context_data(workspace_id).await?;

        if entities.is_empty() {
            return Ok(String::new());
        }

        Ok(build_tree(&entities, &relations))
    }

    // ── Tag Generation ──

    /// Generate 3-5 concise tags for a piece of content using a lightweight LLM call.
    pub async fn generate_tags(&self, content: &str) -> Result<Vec<String>> {
        generate_tags_inner(content).await
    }
}

// ── Background Parse ──

/// Run the full parse pipeline in the background.
///
/// 1. Mark job as "running" and doc as "parsing"
/// 2. Read document content from DB (already have it from trigger_parse)
/// 3. Call LLM to extract entities and relations
/// 4. Get previous entities/relations for diff calculation
/// 5. Delete old entities+relations for this document, upsert new ones
/// 6. Compute ParseDiff
/// 7. Update job to "completed" with result_summary
/// 8. On failure: update job to "failed" with error_message
/// 9. After success: generate tags for the document
async fn run_parse(
    repo: Arc<dyn KnowledgeRepository>,
    document_id: &str,
    workspace_id: &str,
    parse_id: &str,
    content: &str,
) {
    // 1. Set job to "running"
    if let Err(e) = repo.update_parse_job(parse_id, "running", None, None).await {
        tracing::error!("Failed to update parse job {} to running: {}", parse_id, e);
        return;
    }

    // 2-3. Call LLM
    let llm_result = call_llm_parse(content, workspace_id, document_id).await;

    let (entities, relations) = match llm_result {
        Ok(er) => er,
        Err(e) => {
            // 8. On failure: update job and document
            let error_msg = e.to_string();
            tracing::error!("Parse failed for document {}: {}", document_id, error_msg);
            let _ = repo.update_parse_job(parse_id, "failed", Some(&error_msg), None).await;
            let _ = repo.update_document(document_id, None, None, None, None, Some("failed")).await;
            return;
        }
    };

    // 4. Get previous entities/relations for diff
    let old_entities = repo.get_entities_by_document(document_id).await.unwrap_or_default();
    let old_relations = repo.get_relations_by_document(document_id).await.unwrap_or_default();

    // 5. Delete old entities+relations, upsert new ones
    let _ = repo.delete_relations_by_document(document_id).await;
    let _ = repo.delete_entities_by_document(document_id).await;

    if let Err(e) = repo.upsert_entities(&entities).await {
        tracing::error!("Failed to upsert entities for document {}: {}", document_id, e);
        let _ = repo.update_parse_job(parse_id, "failed", Some(&e.to_string()), None).await;
        let _ = repo.update_document(document_id, None, None, None, None, Some("failed")).await;
        return;
    }

    if let Err(e) = repo.upsert_relations(&relations).await {
        tracing::error!("Failed to upsert relations for document {}: {}", document_id, e);
        let _ = repo.update_parse_job(parse_id, "failed", Some(&e.to_string()), None).await;
        let _ = repo.update_document(document_id, None, None, None, None, Some("failed")).await;
        return;
    }

    // 6. Compute diff
    let diff = compute_parse_diff(&old_entities, &old_relations, &entities, &relations);

    let summary = ParseResultSummary {
        entity_count: entities.len(),
        relation_count: relations.len(),
        diff: Some(diff),
    };

    let summary_json = match serde_json::to_string(&summary) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("Failed to serialize parse result summary: {}", e);
            return;
        }
    };

    // 7. Update job to "completed" and doc to "parsed"
    if let Err(e) = repo.update_parse_job(parse_id, "completed", None, Some(&summary_json)).await {
        tracing::error!("Failed to update parse job {} to completed: {}", parse_id, e);
    }

    if let Err(e) = repo.update_document(document_id, None, None, None, None, Some("parsed")).await
    {
        tracing::error!("Failed to update document {} status to parsed: {}", document_id, e);
    }

    // 9. After success: generate tags for the document
    match generate_tags_inner(content).await {
        Ok(tags) => {
            if !tags.is_empty() {
                let tags_str = tags.join(",");
                if let Err(e) =
                    repo.update_document(document_id, None, None, None, Some(&tags_str), None).await
                {
                    tracing::error!(
                        "Failed to update document {} tags after parse: {}",
                        document_id,
                        e
                    );
                }
            }
        }
        Err(e) => {
            // Tag generation failure is non-fatal
            tracing::warn!("Tag generation failed for document {}: {}", document_id, e);
        }
    }
}

// ── LLM Calls ──

/// Call the LLM to parse document content and extract entities + relations.
async fn call_llm_parse(
    content: &str,
    workspace_id: &str,
    document_id: &str,
) -> Result<(Vec<KnowledgeEntity>, Vec<KnowledgeRelation>)> {
    let system_prompt = build_parse_system_prompt();
    let user_message = format!("<user_document>\n{}\n</user_document>", content);

    let response = retry_llm_call(&system_prompt, &user_message, 0.1).await?;
    let parsed = parse_llm_json_response(&response, workspace_id, document_id)?;

    Ok(parsed)
}

/// Call the LLM for a lightweight preview parse (shorter timeout, simpler prompt).
async fn call_llm_preview(
    content: &str,
    workspace_id: &str,
) -> Result<(Vec<KnowledgeEntity>, Vec<KnowledgeRelation>)> {
    let system_prompt = build_preview_system_prompt();
    let user_message = format!("<user_document>\n{}\n</user_document>", content);

    let response =
        llm_call_with_timeout(&system_prompt, &user_message, 0.1, Duration::from_secs(5)).await?;
    let parsed = parse_llm_json_response(&response, workspace_id, "")?;

    Ok(parsed)
}

/// Retry LLM call with exponential backoff.
///
/// - 3 total attempts
/// - Backoff: 1s, 2s
/// - Timeout: 10s per attempt
/// - Network errors -> retry; Auth errors (401/403) -> don't retry
async fn retry_llm_call(
    system_prompt: &str,
    user_message: &str,
    temperature: f64,
) -> Result<String> {
    let max_attempts: u32 = 3;
    let backoffs = [Duration::from_secs(0), Duration::from_secs(1), Duration::from_secs(2)];
    let timeout = Duration::from_secs(10);

    let mut last_error: Option<String> = None;

    for attempt in 0..max_attempts {
        if attempt > 0 {
            let delay = backoffs[attempt as usize];
            tracing::info!("LLM call retry {}/{} after {:?}", attempt + 1, max_attempts, delay);
            tokio::time::sleep(delay).await;
        }

        match llm_call_with_timeout(system_prompt, user_message, temperature, timeout).await {
            Ok(response) => return Ok(response),
            Err(e) => {
                let err_str = e.to_string();

                // Don't retry on auth errors
                if err_str.contains("401")
                    || err_str.contains("403")
                    || err_str.contains("unauthorized")
                    || err_str.contains("Unauthorized")
                {
                    return Err(Error::Internal(format!("LLM auth error: {}", err_str)));
                }

                tracing::warn!(
                    "LLM call attempt {}/{} failed: {}",
                    attempt + 1,
                    max_attempts,
                    err_str
                );
                last_error = Some(err_str);
            }
        }
    }

    Err(Error::Internal(format!(
        "LLM call failed after {} attempts: {}",
        max_attempts,
        last_error.unwrap_or_else(|| "unknown error".to_string())
    )))
}

/// Make a single LLM call with timeout.
async fn llm_call_with_timeout(
    system_prompt: &str,
    user_message: &str,
    temperature: f64,
    timeout: Duration,
) -> Result<String> {
    let auth_token = crate::shared::config::get()
        .minimax
        .as_ref()
        .map(|m| m.auth_token.clone())
        .ok_or_else(|| Error::ConfigError("minimax.auth_token not configured".to_string()))?;

    let model = crate::shared::config::get()
        .minimax
        .as_ref()
        .map(|m| m.model.clone())
        .unwrap_or_else(|| "MiniMax-M2.7-highspeed".to_string());

    let provider = zeroclaw::providers::create_provider("minimaxi", Some(&auth_token))
        .map_err(|e| Error::Internal(format!("failed to create LLM provider: {}", e)))?;

    let chat_future =
        provider.chat_with_system(Some(system_prompt), user_message, &model, Some(temperature));

    match tokio::time::timeout(timeout, chat_future).await {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(e)) => Err(Error::Internal(format!("LLM call error: {}", e))),
        Err(_elapsed) => Err(Error::Internal("LLM call timed out".to_string())),
    }
}

// ── System Prompts ──

/// Build the detailed system prompt for document parsing.
fn build_parse_system_prompt() -> String {
    r#"你是一个知识图谱解析专家。分析用户文档，提取实体和关系。

## 实体类型

- space(空间): 物理或逻辑空间，如"车间A"、"办公楼B"、"数据中心"
- device(设备): IoT 设备，如传感器、执行器、网关、控制器
- functional(功能): 软件功能、服务、API、模块
- custom:xxx: 自定义类型，前缀 custom: 后跟类型名，如 custom:protocol

## 关系类型

- contains(包含): 一个空间包含子空间或设备
- manages(管理): 一个设备/系统管理另一个设备
- monitors(监控): 一个设备监控另一个设备或空间
- references(引用): 一个实体引用另一个实体的信息
- connects_to(连接): 两个设备之间物理或逻辑连接

## 标签约定

- 每个实体可包含 1-5 个标签
- 标签应简洁，优先使用中文
- 标签应反映实体的类别、用途或关键属性

## 输出格式

严格返回 JSON，不要包含任何额外文字、注释或 markdown 标记：

{
  "entities": [
    {
      "entity_type": "device",
      "name": "温度传感器A",
      "description": "安装在车间A的温度传感器，通过MQTT上报数据",
      "tags": ["传感器", "温度", "MQTT"],
      "confidence": 0.95
    }
  ],
  "relations": [
    {
      "source_name": "车间A",
      "target_name": "温度传感器A",
      "relation_type": "contains",
      "confidence": 0.9
    }
  ]
}

## 字段说明

- entity_type: 必须是上述列出的类型之一
- name: 实体名称，应简洁唯一
- description: 简短描述，提取文档中的关键信息
- tags: 1-5 个中文标签
- confidence: 0.0-1.0，表示你对该提取结果的置信度
- source_name: 关系源实体名称（必须匹配 entities 中的某个 name）
- target_name: 关系目标实体名称（必须匹配 entities 中的某个 name）
- relation_type: 必须是上述列出的关系类型之一

## 注意事项

1. 实体名称应保持稳定，同一实体多次解析应使用相同名称
2. 优先提取文档中明确提到的实体，不要臆造
3. 如果 confidence 低于 0.7，说明关系不确定
4. 忽略文档中与 IoT/物联网无关的元信息
"#
    .to_string()
}

/// Build a lighter system prompt for preview parsing.
fn build_preview_system_prompt() -> String {
    r#"你是一个知识图谱解析专家。快速分析文档，提取实体和关系。

实体类型: space(空间), device(设备), functional(功能), custom:xxx(自定义)
关系类型: contains(包含), manages(管理), monitors(监控), references(引用), connects_to(连接)

严格返回 JSON:
{
  "entities": [
    {"entity_type": "device", "name": "实体名", "description": "简述", "tags": ["标签"], "confidence": 0.9}
  ],
  "relations": [
    {"source_name": "源实体", "target_name": "目标实体", "relation_type": "contains", "confidence": 0.9}
  ]
}

只返回 JSON，不要任何解释。"#
        .to_string()
}

// ── JSON Response Parsing ──

/// Parsed entity with name reference (before ID assignment).
#[derive(Debug, serde::Deserialize)]
struct LlmEntity {
    entity_type: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_confidence")]
    confidence: f32,
    #[serde(default)]
    properties: Option<serde_json::Value>,
}

fn default_confidence() -> f32 {
    0.9
}

/// Parsed relation with name references (before ID assignment).
#[derive(Debug, serde::Deserialize)]
struct LlmRelation {
    source_name: String,
    target_name: String,
    relation_type: String,
    #[serde(default = "default_confidence")]
    confidence: f32,
    #[serde(default)]
    properties: Option<serde_json::Value>,
}

/// Parse the LLM JSON response into entity and relation types.
///
/// Handles common LLM output quirks:
/// - JSON wrapped in ```json ... ``` code blocks
/// - Extra text before/after the JSON object
/// - Malformed JSON — extracts what it can
fn parse_llm_json_response(
    response: &str,
    workspace_id: &str,
    document_id: &str,
) -> Result<(Vec<KnowledgeEntity>, Vec<KnowledgeRelation>)> {
    let json_str = extract_json_from_response(response);

    let value: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| {
        Error::SerializationError(format!("failed to parse LLM JSON response: {}", e))
    })?;

    let now = chrono::Utc::now().to_rfc3339();

    // Parse entities
    let mut entities: Vec<KnowledgeEntity> = Vec::new();
    let mut name_to_id: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    if let Some(entities_array) = value.get("entities").and_then(|v| v.as_array()) {
        for item in entities_array {
            let llm_entity: LlmEntity = match serde_json::from_value(item.clone()) {
                Ok(e) => e,
                Err(_) => {
                    // Try to extract what we can with low confidence
                    let name =
                        item.get("name").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                    let entity_type = item
                        .get("entity_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("functional")
                        .to_string();

                    let entity_id = format!("ent-{}", uuid::Uuid::new_v4());
                    name_to_id.insert(name.clone(), entity_id.clone());

                    entities.push(KnowledgeEntity {
                        id: entity_id,
                        workspace_id: workspace_id.to_string(),
                        source_document_id: document_id.to_string(),
                        entity_type,
                        name,
                        description: None,
                        properties: serde_json::Value::Object(serde_json::Map::new()),
                        tags: Vec::new(),
                        file_ids: Vec::new(),
                        device_id: None,
                        confidence: 0.3,
                        created_at: now.clone(),
                        updated_at: now.clone(),
                    });
                    continue;
                }
            };

            let entity_id = format!("ent-{}", uuid::Uuid::new_v4());
            name_to_id.insert(llm_entity.name.clone(), entity_id.clone());

            entities.push(KnowledgeEntity {
                id: entity_id,
                workspace_id: workspace_id.to_string(),
                source_document_id: document_id.to_string(),
                entity_type: llm_entity.entity_type,
                name: llm_entity.name,
                description: llm_entity.description,
                properties: llm_entity
                    .properties
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                tags: llm_entity.tags,
                file_ids: Vec::new(),
                device_id: None,
                confidence: llm_entity.confidence,
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        }
    }

    // Parse relations (map entity names to IDs)
    let mut relations: Vec<KnowledgeRelation> = Vec::new();

    if let Some(relations_array) = value.get("relations").and_then(|v| v.as_array()) {
        for item in relations_array {
            let llm_relation: LlmRelation = match serde_json::from_value(item.clone()) {
                Ok(r) => r,
                Err(_) => {
                    // Skip relations we can't parse
                    continue;
                }
            };

            // Map names to entity IDs
            let source_id = match name_to_id.get(&llm_relation.source_name) {
                Some(id) => id.clone(),
                None => {
                    // Try to find entity by name in the entities we just created
                    match entities.iter().find(|e| e.name == llm_relation.source_name) {
                        Some(e) => e.id.clone(),
                        None => {
                            tracing::warn!(
                                "Relation references unknown source entity: {}",
                                llm_relation.source_name
                            );
                            continue;
                        }
                    }
                }
            };

            let target_id = match name_to_id.get(&llm_relation.target_name) {
                Some(id) => id.clone(),
                None => match entities.iter().find(|e| e.name == llm_relation.target_name) {
                    Some(e) => e.id.clone(),
                    None => {
                        tracing::warn!(
                            "Relation references unknown target entity: {}",
                            llm_relation.target_name
                        );
                        continue;
                    }
                },
            };

            relations.push(KnowledgeRelation {
                id: format!("rel-{}", uuid::Uuid::new_v4()),
                workspace_id: workspace_id.to_string(),
                source_entity_id: source_id,
                target_entity_id: target_id,
                relation_type: llm_relation.relation_type,
                properties: llm_relation
                    .properties
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                confidence: llm_relation.confidence,
            });
        }
    }

    Ok((entities, relations))
}

/// Extract JSON from an LLM response that may contain markdown or extra text.
fn extract_json_from_response(response: &str) -> String {
    // Try to extract from ```json ... ``` code block
    if let Some(start) = response.find("```json") {
        let after_start = &response[start + 7..];
        if let Some(end) = after_start.find("```") {
            return after_start[..end].trim().to_string();
        }
    }

    // Try to extract from ``` ... ``` code block
    if let Some(start) = response.find("```") {
        let after_start = &response[start + 3..];
        if let Some(end) = after_start.find("```") {
            return after_start[..end].trim().to_string();
        }
    }

    // Try to find a JSON object directly
    if let Some(start) = response.find('{') {
        let after_start = &response[start..];
        // Find the matching closing brace
        let mut depth = 0;
        let mut end_idx = 0;
        for (i, ch) in after_start.char_indices() {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    end_idx = i + 1;
                    break;
                }
            }
        }
        if end_idx > 0 {
            return after_start[..end_idx].to_string();
        }
    }

    // Return as-is as a fallback
    response.to_string()
}

// ── Diff Calculation ──

/// Compute the parse diff between old and new entities/relations.
fn compute_parse_diff(
    old_entities: &[KnowledgeEntity],
    old_relations: &[KnowledgeRelation],
    new_entities: &[KnowledgeEntity],
    new_relations: &[KnowledgeRelation],
) -> ParseDiff {
    // Build old name sets
    let old_entity_names: std::collections::HashSet<&str> =
        old_entities.iter().map(|e| e.name.as_str()).collect();
    let new_entity_names: std::collections::HashSet<&str> =
        new_entities.iter().map(|e| e.name.as_str()).collect();

    // Count added/removed entities by name
    let added = new_entity_names.difference(&old_entity_names).count();
    let removed = old_entity_names.difference(&new_entity_names).count();

    // Count modified entities (same name, different description)
    let mut modified = 0;
    for old_e in old_entities {
        if new_entity_names.contains(old_e.name.as_str())
            && let Some(new_e) = new_entities.iter().find(|e| e.name == old_e.name)
            && (old_e.description != new_e.description
                || old_e.entity_type != new_e.entity_type
                || old_e.tags != new_e.tags)
        {
            modified += 1;
        }
    }

    // Add relation changes to modified count
    let old_rel_count = old_relations.len();
    let new_rel_count = new_relations.len();
    if new_rel_count > old_rel_count {
        // Not purely additive for entities since relations might not have names
    }

    ParseDiff { added, removed, modified }
}

// ── Tree Building ──

/// Build a tree-structured text representation of entities and relations.
///
/// Uses "contains" relations to determine hierarchy. Root entities are those
/// NOT targeted by any "contains" relation.
fn build_tree(entities: &[KnowledgeEntity], relations: &[KnowledgeRelation]) -> String {
    // Find entity IDs that are targets of "contains" relations
    let contained_ids: std::collections::HashSet<&str> = relations
        .iter()
        .filter(|r| r.relation_type == "contains")
        .map(|r| r.target_entity_id.as_str())
        .collect();

    // Find root entities (not contained by anything)
    let roots: Vec<&KnowledgeEntity> =
        entities.iter().filter(|e| !contained_ids.contains(e.id.as_str())).collect();

    if roots.is_empty() {
        // Fallback: if no contains relations, list all entities flat
        let mut output = String::new();
        for e in entities {
            output.push_str(&format!("- {} ({})\n", e.name, e.entity_type));
        }
        return output;
    }

    let mut output = String::new();

    for (i, root) in roots.iter().enumerate() {
        let is_last_root = i == roots.len() - 1;
        render_tree_node(&mut output, entities, relations, root, "", is_last_root);
    }

    output
}

/// Recursively render a tree node and its children.
fn render_tree_node(
    output: &mut String,
    entities: &[KnowledgeEntity],
    relations: &[KnowledgeRelation],
    entity: &KnowledgeEntity,
    prefix: &str,
    is_last: bool,
) {
    let connector = if prefix.is_empty() {
        ""
    } else if is_last {
        "└── "
    } else {
        "├── "
    };

    let type_label = match entity.entity_type.as_str() {
        "space" => "[空间]",
        "device" if entity.device_id.is_some() => "[设备]",
        "device" => "[设备]",
        "functional" => "[功能]",
        other if other.starts_with("custom:") => &other[7..],
        _ => "",
    };

    output.push_str(&format!("{}{}{} {}\n", prefix, connector, entity.name, type_label));

    if let Some(ref desc) = entity.description
        && !desc.is_empty()
    {
        let desc_prefix = if prefix.is_empty() || is_last { "    " } else { "│   " };
        let short_desc = if desc.len() > 80 {
            let end = desc.floor_char_boundary(80);
            format!("{}...", &desc[..end])
        } else {
            desc.clone()
        };
        output.push_str(&format!("{}{}  {}\n", prefix, desc_prefix, short_desc));
    }

    // Find children (entities contained by this entity)
    let children: Vec<&KnowledgeEntity> = relations
        .iter()
        .filter(|r| r.relation_type == "contains" && r.source_entity_id == entity.id)
        .filter_map(|r| entities.iter().find(|e| e.id == r.target_entity_id))
        .collect();

    let child_prefix =
        if is_last { format!("{}    ", prefix) } else { format!("{}│   ", prefix) };

    for (i, child) in children.iter().enumerate() {
        let is_last_child = i == children.len() - 1;
        render_tree_node(output, entities, relations, child, &child_prefix, is_last_child);
    }
}

// ── Tag Generation ──

/// Generate 3-5 tags for document content using a lightweight LLM call.
async fn generate_tags_inner(content: &str) -> Result<Vec<String>> {
    let prompt = format!(
        "根据以下文档内容，生成 3-5 个简洁的中文标签。只返回逗号分隔的标签，不要任何解释。\n\n文档：\n{}",
        // Truncate long content for tag generation
        if content.len() > 2000 {
            let end = content.floor_char_boundary(2000);
            &content[..end]
        } else {
            content
        }
    );

    let provider = zeroclaw::providers::create_provider(
        "minimaxi",
        Some(
            &crate::shared::config::get()
                .minimax
                .as_ref()
                .map(|m| m.auth_token.clone())
                .unwrap_or_default(),
        ),
    )
    .map_err(|e| Error::Internal(format!("failed to create LLM provider: {}", e)))?;

    let model = crate::shared::config::get()
        .minimax
        .as_ref()
        .map(|m| m.model.clone())
        .unwrap_or_else(|| "MiniMax-M2.7-highspeed".to_string());

    let raw = provider
        .chat_with_system(None, &prompt, &model, Some(0.3))
        .await
        .map_err(|e| Error::Internal(format!("tag generation failed: {}", e)))?;

    let tags: Vec<String> = raw
        .split([',', '，', '、', '\n'])
        .map(|t| t.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|t| !t.is_empty() && t.len() < 20)
        .take(5)
        .collect();

    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_from_code_block() {
        let input = "```json\n{\"entities\": []}\n```";
        let result = extract_json_from_response(input);
        assert_eq!(result, "{\"entities\": []}");
    }

    #[test]
    fn test_extract_json_from_plain_block() {
        let input = "```\n{\"entities\": []}\n```";
        let result = extract_json_from_response(input);
        assert_eq!(result, "{\"entities\": []}");
    }

    #[test]
    fn test_extract_json_direct_object() {
        let input = "Here is the result: {\"entities\": [{\"name\": \"test\"}]} done.";
        let result = extract_json_from_response(input);
        assert_eq!(result, "{\"entities\": [{\"name\": \"test\"}]}");
    }

    #[test]
    fn test_extract_json_nested() {
        let input = r#"{"entities": [{"name": "test", "tags": ["a", "b"]}], "relations": [{"source_name": "test", "target_name": "other", "relation_type": "contains"}]}"#;
        let result = extract_json_from_response(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_build_tree_simple() {
        let entities = vec![
            KnowledgeEntity {
                id: "e1".to_string(),
                workspace_id: "ws1".to_string(),
                source_document_id: "d1".to_string(),
                entity_type: "space".to_string(),
                name: "车间A".to_string(),
                description: Some("主要生产车间".to_string()),
                properties: serde_json::json!({}),
                tags: vec![],
                file_ids: vec![],
                device_id: None,
                confidence: 0.9,
                created_at: "2024-01-01".to_string(),
                updated_at: "2024-01-01".to_string(),
            },
            KnowledgeEntity {
                id: "e2".to_string(),
                workspace_id: "ws1".to_string(),
                source_document_id: "d1".to_string(),
                entity_type: "device".to_string(),
                name: "温度传感器A".to_string(),
                description: Some("MQTT温度传感器".to_string()),
                properties: serde_json::json!({}),
                tags: vec![],
                file_ids: vec![],
                device_id: None,
                confidence: 0.9,
                created_at: "2024-01-01".to_string(),
                updated_at: "2024-01-01".to_string(),
            },
        ];

        let relations = vec![KnowledgeRelation {
            id: "r1".to_string(),
            workspace_id: "ws1".to_string(),
            source_entity_id: "e1".to_string(),
            target_entity_id: "e2".to_string(),
            relation_type: "contains".to_string(),
            properties: serde_json::json!({}),
            confidence: 0.9,
        }];

        let tree = build_tree(&entities, &relations);
        assert!(tree.contains("车间A"));
        assert!(tree.contains("温度传感器A"));
        // e2 is contained by e1, should be under e1
        assert!(tree.contains("├── ") || tree.contains("└── "));
    }

    #[test]
    fn test_build_tree_empty() {
        let tree = build_tree(&[], &[]);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_compute_parse_diff() {
        let old_entities = vec![KnowledgeEntity {
            id: "e1".to_string(),
            workspace_id: "ws1".to_string(),
            source_document_id: "d1".to_string(),
            entity_type: "device".to_string(),
            name: "传感器A".to_string(),
            description: Some("旧描述".to_string()),
            properties: serde_json::json!({}),
            tags: vec![],
            file_ids: vec![],
            device_id: None,
            confidence: 0.9,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
        }];
        let old_relations: Vec<KnowledgeRelation> = vec![];

        let new_entities = vec![KnowledgeEntity {
            id: "e2".to_string(),
            workspace_id: "ws1".to_string(),
            source_document_id: "d1".to_string(),
            entity_type: "device".to_string(),
            name: "传感器B".to_string(),
            description: Some("新设备".to_string()),
            properties: serde_json::json!({}),
            tags: vec![],
            file_ids: vec![],
            device_id: None,
            confidence: 0.9,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
        }];
        let new_relations: Vec<KnowledgeRelation> = vec![];

        let diff = compute_parse_diff(&old_entities, &old_relations, &new_entities, &new_relations);
        assert_eq!(diff.added, 1); // 传感器B is new
        assert_eq!(diff.removed, 1); // 传感器A is removed
        assert_eq!(diff.modified, 0);
    }

    #[test]
    fn test_compute_parse_diff_modified() {
        let old_entities = vec![KnowledgeEntity {
            id: "e1".to_string(),
            workspace_id: "ws1".to_string(),
            source_document_id: "d1".to_string(),
            entity_type: "device".to_string(),
            name: "传感器A".to_string(),
            description: Some("旧描述".to_string()),
            properties: serde_json::json!({}),
            tags: vec!["旧标签".to_string()],
            file_ids: vec![],
            device_id: None,
            confidence: 0.9,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
        }];

        let old_relations: Vec<KnowledgeRelation> = vec![];

        let new_entities = vec![KnowledgeEntity {
            id: "e2".to_string(),
            workspace_id: "ws1".to_string(),
            source_document_id: "d1".to_string(),
            entity_type: "device".to_string(),
            name: "传感器A".to_string(),
            description: Some("新描述".to_string()),
            properties: serde_json::json!({}),
            tags: vec!["新标签".to_string()],
            file_ids: vec![],
            device_id: None,
            confidence: 0.9,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
        }];

        let new_relations: Vec<KnowledgeRelation> = vec![];

        let diff = compute_parse_diff(&old_entities, &old_relations, &new_entities, &new_relations);
        assert_eq!(diff.added, 0);
        assert_eq!(diff.removed, 0);
        assert_eq!(diff.modified, 1);
    }
}
