use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;

use crate::{
    api::AppState,
    dto::{
        entity::tag::{
            BatchTagBindingRequest, CreateTagBindingRequest, CreateTagRequest, Tag, TagBinding,
            TagQuery, UpdateTagRequest,
        },
        response::{api_response::ApiResponse, builder::ApiResponseBuilder, PaginatedResponse, PaginationInfo},
    },
    shared::security::jwt::Claims,
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
        .route("/:id", get(get_tag).put(update_tag).delete(delete_tag))
        .route("/search", get(search_tags))
        .route("/stats", get(get_tag_stats))
        .route("/bindings", post(create_tag_binding).delete(delete_tag_binding))
        .route("/bindings/batch", post(batch_create_bindings).delete(batch_delete_bindings))
        .route("/bindings/target/:target_id", get(get_target_bindings))
        .route("/bindings/tag/:tag_id", get(get_tag_bindings))
}

/// 获取标签列表
async fn list_tags(
    Query(query): Query<TagListQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<PaginatedResponse<Tag>>>, StatusCode> {
    let tag_query = TagQuery {
        name: query.name.clone(),
        tag_type: query.tag_type.clone(),
        target_id: None,
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
            let total_pages = if page_size > 0 {
                ((total as f64) / (page_size as f64)).ceil() as u32
            } else {
                0
            };
            Ok(ApiResponseBuilder::success(PaginatedResponse {
                data: tags,
                pagination: PaginationInfo {
                    page,
                    page_size,
                    total_pages,
                    total_count,
                },
            }))
        }
        Err(e) => {
            tracing::error!("Failed to fetch tags: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 根据ID获取标签
async fn get_tag(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Tag>>, StatusCode> {
    match state.tag_service.find_tag_by_id(&id).await {
        Ok(Some(tag)) => Ok(ApiResponseBuilder::success(tag)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to fetch tag {}: {}", id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 创建标签
async fn create_tag(
    claims: Claims,
    State(state): State<AppState>,
    Json(request): Json<CreateTagRequest>,
) -> Result<Json<ApiResponse<Tag>>, StatusCode> {
    match state.tag_service.tag_exists_by_name_and_type(&request.name, &request.tag_type).await {
        Ok(true) => return Err(StatusCode::CONFLICT),
        Ok(false) => {}
        Err(e) => {
            tracing::error!("Failed to check tag name existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    match state.tag_service.create_tag(&request, &claims.user_id).await {
        Ok(tag) => Ok(ApiResponseBuilder::success_with_message(tag, "Tag created successfully")),
        Err(e) => {
            tracing::error!("Failed to create tag: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 更新标签
async fn update_tag(
    Path(id): Path<String>,
    State(state): State<AppState>,
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

        match state.tag_service
            .tag_exists_by_name_and_type_exclude_id(name, &current_tag.tag_type, &id)
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

    match state.tag_service.update_tag(&id, &request).await {
        Ok(tag) => Ok(ApiResponseBuilder::success_with_message(tag, "Tag updated successfully")),
        Err(crate::shared::error::Error::NotFound) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to update tag {}: {}", id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 删除标签
async fn delete_tag(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    match state.tag_service.delete_tag(&id).await {
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

/// 搜索标签
async fn search_tags(
    Query(query): Query<TagListQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<PaginatedResponse<Tag>>>, StatusCode> {
    let tag_query = TagQuery {
        name: query.name.clone(),
        tag_type: query.tag_type.clone(),
        target_id: None,
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
            let total_pages = if page_size > 0 {
                ((total as f64) / (page_size as f64)).ceil() as u32
            } else {
                0
            };
            Ok(ApiResponseBuilder::success(PaginatedResponse {
                data: tags,
                pagination: PaginationInfo {
                    page,
                    page_size,
                    total_pages,
                    total_count,
                },
            }))
        }
        Err(e) => {
            tracing::error!("Failed to search tags: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 获取标签统计信息
async fn get_tag_stats(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    let tag_query = TagQuery::default();

    match state.tag_service.count_tags(&tag_query).await {
        Ok(total) => {
            let stats = serde_json::json!({
                "total": total,
                "by_type": {
                    "device": 0, // TODO: 实现按类型统计
                    "app": 0
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

/// 创建标签绑定
async fn create_tag_binding(
    claims: Claims,
    State(state): State<AppState>,
    Json(request): Json<CreateTagBindingRequest>,
) -> Result<Json<ApiResponse<TagBinding>>, StatusCode> {
    match state.tag_service.binding_exists(&request.tag_id, &request.target_id).await {
        Ok(true) => return Err(StatusCode::CONFLICT),
        Ok(false) => {}
        Err(e) => {
            tracing::error!("Failed to check tag binding existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    match state.tag_service.create_binding(&request, &claims.user_id).await {
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

/// 删除标签绑定
async fn delete_tag_binding(
    Query(query): Query<DeleteTagBindingQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    match state.tag_service
        .delete_binding_by_tag_and_target(&query.tag_id, &query.target_id)
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

/// 批量创建标签绑定
async fn batch_create_bindings(
    claims: Claims,
    State(state): State<AppState>,
    Json(request): Json<BatchTagBindingRequest>,
) -> Result<Json<ApiResponse<Vec<TagBinding>>>, StatusCode> {
    let bindings: Vec<CreateTagBindingRequest> = request
        .tag_ids
        .into_iter()
        .map(|tag_id| CreateTagBindingRequest { tag_id, target_id: request.target_id.clone(), target_type: request.target_type.clone() })
        .collect();

    match state.tag_service.create_bindings_batch(&bindings, &claims.user_id).await {
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

/// 批量删除标签绑定
async fn batch_delete_bindings(
    Query(query): Query<TagBindingQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    match state.tag_service.delete_all_bindings_by_target_id(&query.target_id).await {
        Ok(_) => {
            Ok(ApiResponseBuilder::success_with_message((), "Tag bindings deleted successfully"))
        }
        Err(e) => {
            tracing::error!("Failed to delete tag bindings: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 获取目标的标签绑定
async fn get_target_bindings(
    Path(target_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<PaginatedResponse<Tag>>>, StatusCode> {
    let page = 1u32;
    let page_size = 100u32;

    let (tags_result, count_result) = tokio::join!(
        state.tag_service.find_tags_by_target_id(&target_id),
        state.tag_service.count_bindings_by_target_id(&target_id),
    );

    match tags_result {
        Ok(tags) => {
            let total = count_result.unwrap_or(0);
            let total_count = total as u64;
            let total_pages = if page_size > 0 {
                ((total as f64) / (page_size as f64)).ceil() as u32
            } else {
                0
            };
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

/// 获取标签的绑定
async fn get_tag_bindings(
    Path(tag_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<PaginatedResponse<TagBinding>>>, StatusCode> {
    let page = 1u32;
    let page_size = 100u32;

    let (bindings_result, count_result) = tokio::join!(
        state.tag_service.find_bindings_by_tag_id(&tag_id),
        state.tag_service.count_bindings_by_tag_id(&tag_id),
    );

    match bindings_result {
        Ok(bindings) => {
            let total = count_result.unwrap_or(0);
            let total_count = total as u64;
            let total_pages = if page_size > 0 {
                ((total as f64) / (page_size as f64)).ceil() as u32
            } else {
                0
            };
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
