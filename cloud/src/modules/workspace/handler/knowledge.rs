// Knowledge graph API handlers

use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use tinyiothub_web::response::{ApiResponseBuilder, PaginatedResponse, PaginationInfo};

use super::super::types::knowledge::*;
use crate::shared::{api_response::ApiResponse, app_state::AppState, security::jwt::Claims};

// ── Helper ──

/// Verify the workspace exists and belongs to the current tenant.
/// Returns the workspace on success, or a Json error response.
macro_rules! verify_workspace_access {
    ($state:expr, $claims:expr, $id:expr) => {{
        match $state.workspace_service.find_by_id(&$id).await {
            Ok(Some(workspace)) => {
                if workspace.tenant_id != $claims.tenant_id {
                    return ApiResponseBuilder::error_with_code(403, "无权访问此工作空间");
                }
                workspace
            }
            Ok(None) => return ApiResponseBuilder::error_with_code(404, "工作空间不存在"),
            Err(e) => {
                tracing::error!("Failed to get workspace: {}", e);
                return ApiResponseBuilder::error("获取工作空间失败");
            }
        }
    }};
}

// ── Document CRUD ──

/// GET /{id}/knowledge/documents
/// List documents with pagination and optional filters.
pub async fn list_documents(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Query(params): Query<KnowledgeDocumentListParams>,
) -> Json<ApiResponse<PaginatedResponse<KnowledgeDocument>>> {
    verify_workspace_access!(state, claims, id);

    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(20).clamp(1, 100);

    match state
        .knowledge_service
        .list_documents(
            &id,
            params.q.as_deref(),
            params.tags.as_deref(),
            params.status.as_deref(),
            page,
            page_size,
        )
        .await
    {
        Ok((docs, total)) => {
            let total_pages =
                ((total as f64) / (page_size as f64)).ceil() as u32;
            ApiResponseBuilder::success(PaginatedResponse {
                data: docs,
                pagination: PaginationInfo {
                    page: page as u32,
                    page_size: page_size as u32,
                    total_pages,
                    total_count: total as u64,
                },
            })
        }
        Err(e) => {
            tracing::error!("Failed to list documents: {}", e);
            ApiResponseBuilder::error("获取文档列表失败")
        }
    }
}

/// POST /{id}/knowledge/documents
/// Create a new knowledge document.
pub async fn create_document(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(payload): Json<CreateKnowledgeDocumentRequest>,
) -> Json<ApiResponse<KnowledgeDocument>> {
    verify_workspace_access!(state, claims, id);

    match state
        .knowledge_service
        .create_document(
            &id,
            payload.title,
            payload.content,
            payload.tags.unwrap_or_default(),
        )
        .await
    {
        Ok(doc) => ApiResponseBuilder::success(doc),
        Err(e) => {
            tracing::error!("Failed to create document: {}", e);
            ApiResponseBuilder::error("创建文档失败")
        }
    }
}

/// GET /{id}/knowledge/documents/{did}
/// Get a single document by ID.
pub async fn get_document(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((id, did)): Path<(String, String)>,
) -> Json<ApiResponse<KnowledgeDocument>> {
    verify_workspace_access!(state, claims, id);

    match state.knowledge_service.get_document(&did).await {
        Ok(Some(doc)) => {
            if doc.workspace_id != id {
                return ApiResponseBuilder::error_with_code(404, "文档不存在");
            }
            ApiResponseBuilder::success(doc)
        }
        Ok(None) => ApiResponseBuilder::error_with_code(404, "文档不存在"),
        Err(e) => {
            tracing::error!("Failed to get document: {}", e);
            ApiResponseBuilder::error("获取文档失败")
        }
    }
}

/// PUT /{id}/knowledge/documents/{did}
/// Update an existing document.
pub async fn update_document(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((id, did)): Path<(String, String)>,
    Json(payload): Json<UpdateKnowledgeDocumentRequest>,
) -> Json<ApiResponse<KnowledgeDocument>> {
    verify_workspace_access!(state, claims, id);

    match state
        .knowledge_service
        .update_document(
            &did,
            payload.title,
            payload.content,
            payload.tags,
        )
        .await
    {
        Ok(Some(doc)) => {
            if doc.workspace_id != id {
                return ApiResponseBuilder::error_with_code(404, "文档不存在");
            }
            ApiResponseBuilder::success(doc)
        }
        Ok(None) => ApiResponseBuilder::error_with_code(404, "文档不存在"),
        Err(e) => {
            tracing::error!("Failed to update document: {}", e);
            ApiResponseBuilder::error("更新文档失败")
        }
    }
}

/// DELETE /{id}/knowledge/documents/{did}
/// Delete a document and its associated entities, relations, and parse jobs.
pub async fn delete_document(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((id, did)): Path<(String, String)>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_workspace_access!(state, claims, id);

    // Verify document exists and belongs to this workspace
    match state.knowledge_service.get_document(&did).await {
        Ok(Some(doc)) => {
            if doc.workspace_id != id {
                return ApiResponseBuilder::error_with_code(404, "文档不存在");
            }
        }
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "文档不存在"),
        Err(e) => {
            tracing::error!("Failed to get document: {}", e);
            return ApiResponseBuilder::error("获取文档失败");
        }
    }

    match state.knowledge_service.delete_document(&did).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"success": true})),
        Err(e) => {
            tracing::error!("Failed to delete document: {}", e);
            ApiResponseBuilder::error("删除文档失败")
        }
    }
}

// ── Parse Pipeline ──

/// POST /{id}/knowledge/documents/{did}/parse
/// Trigger an async parse job for a document.
pub async fn trigger_parse(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((id, did)): Path<(String, String)>,
) -> Json<ApiResponse<serde_json::Value>> {
    verify_workspace_access!(state, claims, id);

    // Verify document exists and belongs to this workspace
    match state.knowledge_service.get_document(&did).await {
        Ok(Some(doc)) => {
            if doc.workspace_id != id {
                return ApiResponseBuilder::error_with_code(404, "文档不存在");
            }
        }
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "文档不存在"),
        Err(e) => {
            tracing::error!("Failed to get document: {}", e);
            return ApiResponseBuilder::error("获取文档失败");
        }
    }

    match state.knowledge_service.trigger_parse(&did, &id).await {
        Ok(job_id) => {
            ApiResponseBuilder::success(serde_json::json!({"job_id": job_id}))
        }
        Err(e) => {
            tracing::error!("Failed to trigger parse: {}", e);
            ApiResponseBuilder::error("启动解析失败")
        }
    }
}

/// POST /{id}/knowledge/documents/{did}/preview
/// Preview parse a document without persisting results.
pub async fn preview_parse(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((id, did)): Path<(String, String)>,
    Json(payload): Json<PreviewParseRequest>,
) -> Json<ApiResponse<PreviewParseResponse>> {
    verify_workspace_access!(state, claims, id);

    // Verify document exists and belongs to this workspace
    match state.knowledge_service.get_document(&did).await {
        Ok(Some(doc)) => {
            if doc.workspace_id != id {
                return ApiResponseBuilder::error_with_code(404, "文档不存在");
            }
        }
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "文档不存在"),
        Err(e) => {
            tracing::error!("Failed to get document: {}", e);
            return ApiResponseBuilder::error("获取文档失败");
        }
    }

    match state.knowledge_service.preview_parse(&payload.content, &id).await {
        Ok(result) => ApiResponseBuilder::success(result),
        Err(e) => {
            tracing::error!("Failed to preview parse: {}", e);
            ApiResponseBuilder::error("预览解析失败")
        }
    }
}

/// GET /{id}/knowledge/parse/{job_id}
/// Get the status of a parse job.
pub async fn get_parse_job(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((id, job_id)): Path<(String, String)>,
) -> Json<ApiResponse<KnowledgeParseJob>> {
    verify_workspace_access!(state, claims, id);

    match state.knowledge_service.get_parse_job(&job_id).await {
        Ok(Some(job)) => {
            // Security: verify the job's document belongs to this workspace
            if let Ok(Some(doc)) = state.knowledge_service.get_document(&job.document_id).await
                && doc.workspace_id != id
            {
                return ApiResponseBuilder::error_with_code(404, "解析任务不存在");
            }
            ApiResponseBuilder::success(job)
        }
        Ok(None) => ApiResponseBuilder::error_with_code(404, "解析任务不存在"),
        Err(e) => {
            tracing::error!("Failed to get parse job: {}", e);
            ApiResponseBuilder::error("获取解析任务失败")
        }
    }
}

// ── Entity Operations ──

/// GET /{id}/knowledge/entities
/// List entities for a workspace with optional filters.
pub async fn list_entities(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Query(params): Query<KnowledgeEntityListParams>,
) -> Json<ApiResponse<Vec<KnowledgeEntity>>> {
    verify_workspace_access!(state, claims, id);

    match state
        .knowledge_service
        .list_entities(&id, params.entity_type.as_deref(), params.tags.as_deref())
        .await
    {
        Ok(entities) => ApiResponseBuilder::success(entities),
        Err(e) => {
            tracing::error!("Failed to list entities: {}", e);
            ApiResponseBuilder::error("获取实体列表失败")
        }
    }
}

/// PUT /{id}/knowledge/entities/{eid}
/// Update an entity with optional fields.
pub async fn update_entity(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((id, eid)): Path<(String, String)>,
    Json(payload): Json<UpdateKnowledgeEntityRequest>,
) -> Json<ApiResponse<KnowledgeEntity>> {
    verify_workspace_access!(state, claims, id);

    // Verify entity exists and belongs to this workspace
    match state.knowledge_service.get_entity(&eid).await {
        Ok(Some(entity)) => {
            if entity.workspace_id != id {
                return ApiResponseBuilder::error_with_code(404, "实体不存在");
            }
        }
        Ok(None) => return ApiResponseBuilder::error_with_code(404, "实体不存在"),
        Err(e) => {
            tracing::error!("Failed to get entity: {}", e);
            return ApiResponseBuilder::error("获取实体失败");
        }
    }

    // Serialize optional fields to strings for the service layer
    let properties_str = payload
        .properties
        .as_ref()
        .map(|p| serde_json::to_string(p).unwrap_or_default());
    let tags_str = payload.tags.as_ref().map(|t| t.join(","));

    // device_id: Option<String> where None means "don't update" and Some(None) means "clear"
    let device_id_param: Option<Option<&str>> =
        payload.device_id.as_ref().map(|inner| inner.as_deref());

    match state
        .knowledge_service
        .update_entity(
            &eid,
            payload.name.as_deref(),
            payload.description.as_deref(),
            payload.entity_type.as_deref(),
            properties_str.as_deref(),
            tags_str.as_deref(),
            device_id_param,
        )
        .await
    {
        Ok(Some(entity)) => ApiResponseBuilder::success(entity),
        Ok(None) => ApiResponseBuilder::error_with_code(404, "实体不存在"),
        Err(e) => {
            tracing::error!("Failed to update entity: {}", e);
            ApiResponseBuilder::error("更新实体失败")
        }
    }
}

// ── Relation Operations ──

/// GET /{id}/knowledge/relations
/// List all relations for a workspace.
pub async fn list_relations(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Json<ApiResponse<Vec<KnowledgeRelation>>> {
    verify_workspace_access!(state, claims, id);

    match state.knowledge_service.list_relations(&id).await {
        Ok(relations) => ApiResponseBuilder::success(relations),
        Err(e) => {
            tracing::error!("Failed to list relations: {}", e);
            ApiResponseBuilder::error("获取关系列表失败")
        }
    }
}

// ── Search ──

/// GET /{id}/knowledge/search
/// Search knowledge graph entities by query text.
pub async fn search_knowledge(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Query(params): Query<KnowledgeSearchParams>,
) -> Json<ApiResponse<Vec<KnowledgeSearchResult>>> {
    verify_workspace_access!(state, claims, id);

    if params.q.is_empty() {
        return ApiResponseBuilder::error_with_code(400, "搜索关键词不能为空");
    }

    let limit = params.limit.unwrap_or(10).clamp(1, 50);

    match state
        .knowledge_service
        .search_knowledge(
            &id,
            &params.q,
            params.entity_type.as_deref(),
            params.tags.as_deref(),
            limit,
        )
        .await
    {
        Ok(results) => ApiResponseBuilder::success(results),
        Err(e) => {
            tracing::error!("Failed to search knowledge: {}", e);
            ApiResponseBuilder::error("搜索知识图谱失败")
        }
    }
}

// ── Context ──

/// GET /{id}/knowledge/context
/// Returns a tree-structured text representation of the workspace knowledge graph.
///
/// This endpoint returns plain text (Content-Type: text/plain) for direct
/// Agent system prompt injection, NOT JSON.
pub async fn get_context(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Manual workspace access check (cannot use macro — returns impl IntoResponse, not Json)
    match state.workspace_service.find_by_id(&id).await {
        Ok(Some(workspace)) => {
            if workspace.tenant_id != claims.tenant_id {
                return (
                    StatusCode::FORBIDDEN,
                    [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                    "无权访问此工作空间".to_string(),
                )
                    .into_response();
            }
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                "工作空间不存在".to_string(),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to get workspace: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                "获取工作空间失败".to_string(),
            )
                .into_response();
        }
    }

    match state.knowledge_service.build_context(&id).await {
        Ok(text) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            text,
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to build knowledge context: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                format!("Error: {}", e),
            )
                .into_response()
        }
    }
}

// File upload is handled by the resources endpoint:
//   POST /workspaces/{id}/resources/upload
// Knowledge entities reference files via entity.file_ids pointing to resource file paths.
