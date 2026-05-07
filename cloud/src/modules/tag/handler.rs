use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
};
use serde::Deserialize;
use tinyiothub_web::response::ApiResponseBuilder;

use super::types::{
    BatchTagBindingRequest, CreateTagBindingRequest, CreateTagRequest, Tag, TagBinding, TagQuery,
    UpdateTagRequest,
};
use crate::shared::{
    api_response::{ApiResponse, PaginatedResponse, PaginationInfo},
    app_state::AppState,
    security::jwt::Claims,
};

#[derive(Debug, Deserialize)]
pub struct TagListQuery {
    #[serde(rename = "type")]
    pub tag_type: Option<String>,
    pub name: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct TagBindingQuery {
    pub target_id: String,
}

#[derive(Debug, Deserialize)]
pub struct DeleteTagBindingQuery {
    pub tag_id: String,
    pub target_id: String,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_tags).post(create_tag))
        .route("/{id}", get(get_tag).put(update_tag).delete(delete_tag))
        .route("/search", get(search_tags))
        .route("/stats", get(get_tag_stats))
        .route("/bindings", post(create_tag_binding).delete(delete_tag_binding))
        .route("/bindings/batch", post(batch_create_bindings).delete(batch_delete_bindings))
        .route("/bindings/target/{target_id}", get(get_target_bindings))
        .route("/bindings/tag/{tag_id}", get(get_tag_bindings))
}

/// Get tag list
pub async fn list_tags(
    Query(query): Query<TagListQuery>,
    State(state): State<AppState>,
    claims: Claims,
) -> Result<Json<ApiResponse<PaginatedResponse<Tag>>>, StatusCode> {
    let tag_query = TagQuery {
        name: query.name.clone(),
        tag_type: query.tag_type.clone(),
        target_id: None,
        tenant_id: Some(claims.tenant_id),
        page: query.page,
        page_size: query.page_size,
    };

    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20);

    let (tags_result, count_result) = tokio::join!(
        state.tag_service.find_all_tags(&tag_query),
        state.tag_service.count_tags(&tag_query),
    );

    match tags_result {
        Ok(tags) => {
            let total = count_result.unwrap_or(0);
            let total_count = total as u64;
            let total_pages =
                if page_size > 0 { ((total as f64) / (page_size as f64)).ceil() as u32 } else { 0 };
            Ok(ApiResponseBuilder::success(PaginatedResponse {
                data: tags,
                pagination: PaginationInfo { page, page_size, total_pages, total_count },
            }))
        }
        Err(e) => {
            tracing::error!("Failed to fetch tags: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get tag by ID
pub async fn get_tag(
    Path(id): Path<String>,
    State(state): State<AppState>,
    claims: Claims,
) -> Result<Json<ApiResponse<Tag>>, StatusCode> {
    match state.tag_service.find_tag_by_id(&id).await {
        Ok(Some(tag)) => {
            if tag.tenant_id.as_ref() == Some(&claims.tenant_id) {
                Ok(ApiResponseBuilder::success(tag))
            } else {
                Err(StatusCode::NOT_FOUND)
            }
        }
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to fetch tag {}: {}", id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Create tag
pub async fn create_tag(
    claims: Claims,
    State(state): State<AppState>,
    Json(request): Json<CreateTagRequest>,
) -> Result<Json<ApiResponse<Tag>>, StatusCode> {
    match state
        .tag_service
        .tag_exists_by_name_and_type(&request.name, &request.tag_type, &claims.tenant_id)
        .await
    {
        Ok(true) => return Err(StatusCode::CONFLICT),
        Ok(false) => {}
        Err(e) => {
            tracing::error!("Failed to check tag name existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    match state.tag_service.create_tag(&request, &claims.user_id, &claims.tenant_id).await {
        Ok(tag) => Ok(ApiResponseBuilder::success_with_message(tag, "Tag created successfully")),
        Err(e) => {
            tracing::error!("Failed to create tag: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Update tag
pub async fn update_tag(
    Path(id): Path<String>,
    State(state): State<AppState>,
    claims: Claims,
    Json(request): Json<UpdateTagRequest>,
) -> Result<Json<ApiResponse<Tag>>, StatusCode> {
    if let Some(name) = &request.name {
        let current_tag = match state.tag_service.find_tag_by_id(&id).await {
            Ok(Some(tag)) => tag,
            Ok(None) => return Err(StatusCode::NOT_FOUND),
            Err(e) => {
                tracing::error!("Failed to fetch current tag: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

        if current_tag.tenant_id.as_ref() != Some(&claims.tenant_id) {
            return Err(StatusCode::NOT_FOUND);
        }

        match state
            .tag_service
            .tag_exists_by_name_and_type_exclude_id(
                name,
                &current_tag.tag_type,
                &id,
                &claims.tenant_id,
            )
            .await
        {
            Ok(true) => return Err(StatusCode::CONFLICT),
            Ok(false) => {}
            Err(e) => {
                tracing::error!("Failed to check tag name existence: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    match state.tag_service.update_tag(&id, &request, &claims.tenant_id).await {
        Ok(tag) => Ok(ApiResponseBuilder::success_with_message(tag, "Tag updated successfully")),
        Err(tinyiothub_core::error::Error::NotFound) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to update tag {}: {}", id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Delete tag
pub async fn delete_tag(
    Path(id): Path<String>,
    State(state): State<AppState>,
    claims: Claims,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    match state.tag_service.delete_tag(&id, &claims.tenant_id).await {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                Ok(ApiResponseBuilder::success_with_message((), "Tag deleted successfully"))
            } else {
                Err(StatusCode::NOT_FOUND)
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete tag {}: {}", id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Search tags
pub async fn search_tags(
    Query(query): Query<TagListQuery>,
    State(state): State<AppState>,
    claims: Claims,
) -> Result<Json<ApiResponse<PaginatedResponse<Tag>>>, StatusCode> {
    let tag_query = TagQuery {
        name: query.name.clone(),
        tag_type: query.tag_type.clone(),
        target_id: None,
        tenant_id: Some(claims.tenant_id),
        page: query.page,
        page_size: query.page_size,
    };

    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20);

    let (tags_result, count_result) = tokio::join!(
        state.tag_service.find_all_tags(&tag_query),
        state.tag_service.count_tags(&tag_query),
    );

    match tags_result {
        Ok(tags) => {
            let total = count_result.unwrap_or(0);
            let total_count = total as u64;
            let total_pages =
                if page_size > 0 { ((total as f64) / (page_size as f64)).ceil() as u32 } else { 0 };
            Ok(ApiResponseBuilder::success(PaginatedResponse {
                data: tags,
                pagination: PaginationInfo { page, page_size, total_pages, total_count },
            }))
        }
        Err(e) => {
            tracing::error!("Failed to search tags: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get tag stats
pub async fn get_tag_stats(
    State(state): State<AppState>,
    claims: Claims,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    let mut tag_query = TagQuery::default();
    tag_query.tenant_id = Some(claims.tenant_id);

    match state.tag_service.find_all_tags(&tag_query).await {
        Ok(tags) => {
            let mut device_count = 0i64;
            let mut app_count = 0i64;
            for tag in &tags {
                match tag.tag_type.as_str() {
                    "device" => device_count += 1,
                    "app" => app_count += 1,
                    _ => {}
                }
            }
            let stats = serde_json::json!({
                "total": tags.len() as i64,
                "by_type": {
                    "device": device_count,
                    "app": app_count
                }
            });
            Ok(ApiResponseBuilder::success(stats))
        }
        Err(e) => {
            tracing::error!("Failed to get tag stats: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Create tag binding (idempotent: returns existing if already exists)
pub async fn create_tag_binding(
    claims: Claims,
    State(state): State<AppState>,
    Json(request): Json<CreateTagBindingRequest>,
) -> Result<Json<ApiResponse<TagBinding>>, StatusCode> {
    match state
        .tag_service
        .find_binding_by_tag_and_target(&request.tag_id, &request.target_id, &claims.tenant_id)
        .await
    {
        Ok(Some(existing)) => {
            return Ok(ApiResponseBuilder::success_with_message(
                existing,
                "Tag binding already exists",
            ));
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("Failed to check tag binding existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    match state.tag_service.create_binding(&request, &claims.user_id, &claims.tenant_id).await {
        Ok(binding) => Ok(ApiResponseBuilder::success_with_message(
            binding,
            "Tag binding created successfully",
        )),
        Err(e) => {
            tracing::error!("Failed to create tag binding: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Delete tag binding
pub async fn delete_tag_binding(
    Query(query): Query<DeleteTagBindingQuery>,
    State(state): State<AppState>,
    claims: Claims,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    match state
        .tag_service
        .delete_binding_by_tag_and_target(&query.tag_id, &query.target_id, &claims.tenant_id)
        .await
    {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                Ok(ApiResponseBuilder::success_with_message((), "Tag binding deleted successfully"))
            } else {
                Err(StatusCode::NOT_FOUND)
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete tag binding: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Batch create tag bindings
pub async fn batch_create_bindings(
    claims: Claims,
    State(state): State<AppState>,
    Json(request): Json<BatchTagBindingRequest>,
) -> Result<Json<ApiResponse<Vec<TagBinding>>>, StatusCode> {
    let bindings: Vec<CreateTagBindingRequest> = request
        .tag_ids
        .into_iter()
        .map(|tag_id| CreateTagBindingRequest {
            tag_id,
            target_id: request.target_id.clone(),
            target_type: request.target_type.clone(),
        })
        .collect();

    match state
        .tag_service
        .create_bindings_batch(&bindings, &claims.user_id, &claims.tenant_id)
        .await
    {
        Ok(created_bindings) => Ok(ApiResponseBuilder::success_with_message(
            created_bindings,
            "Tag bindings created successfully",
        )),
        Err(e) => {
            tracing::error!("Failed to create tag bindings: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Batch delete tag bindings
pub async fn batch_delete_bindings(
    Query(query): Query<TagBindingQuery>,
    State(state): State<AppState>,
    claims: Claims,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    match state
        .tag_service
        .delete_all_bindings_by_target_id(&query.target_id, &claims.tenant_id)
        .await
    {
        Ok(_) => {
            Ok(ApiResponseBuilder::success_with_message((), "Tag bindings deleted successfully"))
        }
        Err(e) => {
            tracing::error!("Failed to delete tag bindings: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get target's tag bindings
pub async fn get_target_bindings(
    Path(target_id): Path<String>,
    State(state): State<AppState>,
    claims: Claims,
) -> Result<Json<ApiResponse<PaginatedResponse<Tag>>>, StatusCode> {
    let page = 1u32;
    let page_size = 100u32;

    let (tags_result, count_result) = tokio::join!(
        state.tag_service.find_tags_by_target_id(&target_id, &claims.tenant_id),
        state.tag_service.count_bindings_by_target_id(&target_id, &claims.tenant_id),
    );

    match tags_result {
        Ok(tags) => {
            let total = count_result.unwrap_or(0);
            let total_count = total as u64;
            let total_pages =
                if page_size > 0 { ((total as f64) / (page_size as f64)).ceil() as u32 } else { 0 };
            Ok(ApiResponseBuilder::success(PaginatedResponse {
                data: tags,
                pagination: PaginationInfo { page, page_size, total_pages, total_count },
            }))
        }
        Err(e) => {
            tracing::error!("Failed to fetch target bindings: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get tag's bindings
pub async fn get_tag_bindings(
    Path(tag_id): Path<String>,
    State(state): State<AppState>,
    claims: Claims,
) -> Result<Json<ApiResponse<PaginatedResponse<TagBinding>>>, StatusCode> {
    let page = 1u32;
    let page_size = 100u32;

    let (bindings_result, count_result) = tokio::join!(
        state.tag_service.find_bindings_by_tag_id(&tag_id, &claims.tenant_id),
        state.tag_service.count_bindings_by_tag_id(&tag_id, &claims.tenant_id),
    );

    match bindings_result {
        Ok(bindings) => {
            let total = count_result.unwrap_or(0);
            let total_count = total as u64;
            let total_pages =
                if page_size > 0 { ((total as f64) / (page_size as f64)).ceil() as u32 } else { 0 };
            Ok(ApiResponseBuilder::success(PaginatedResponse {
                data: bindings,
                pagination: PaginationInfo { page, page_size, total_pages, total_count },
            }))
        }
        Err(e) => {
            tracing::error!("Failed to fetch tag bindings: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
