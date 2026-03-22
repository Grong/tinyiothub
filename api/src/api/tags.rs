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
        response::{api_response::ApiResponse, builder::ApiResponseBuilder},
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
        .route(
            "/bindings",
            post(create_tag_binding).delete(delete_tag_binding),
        )
        .route(
            "/bindings/batch",
            post(batch_create_bindings).delete(batch_delete_bindings),
        )
        .route("/bindings/target/:target_id", get(get_target_bindings))
        .route("/bindings/tag/:tag_id", get(get_tag_bindings))
}

/// 获取标签列表
async fn list_tags(
    Query(query): Query<TagListQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<Tag>>>, StatusCode> {
    let tag_query = TagQuery {
        name: query.name,
        tag_type: query.tag_type,
        target_id: None,
        page: query.page,
        page_size: query.page_size,
    };

    match Tag::find_all(state.database(), &tag_query).await {
        Ok(tags) => Ok(ApiResponseBuilder::success(tags)),
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
    match Tag::find_by_id(state.database(), &id).await {
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
    // 检查标签名称是否已存在（在同一类型下）
    match Tag::exists_by_name_and_type(state.database(), &request.name, &request.tag_type).await {
        Ok(true) => return Err(StatusCode::CONFLICT),
        Ok(false) => {}
        Err(e) => {
            tracing::error!("Failed to check tag name existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    match Tag::create(state.database(), &request, &claims.user_id).await {
        Ok(tag) => Ok(ApiResponseBuilder::success_with_message(
            tag,
            "Tag created successfully",
        )),
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
    // 如果更新名称，检查是否与其他标签冲突
    if let Some(name) = &request.name {
        // 先获取当前标签信息以获取类型
        let current_tag = match Tag::find_by_id(state.database(), &id).await {
            Ok(Some(tag)) => tag,
            Ok(None) => return Err(StatusCode::NOT_FOUND),
            Err(e) => {
                tracing::error!("Failed to fetch current tag: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

        match Tag::exists_by_name_and_type_exclude_id(
            state.database(),
            name,
            &current_tag.tag_type,
            &id,
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

    match Tag::update(state.database(), &id, &request).await {
        Ok(tag) => Ok(ApiResponseBuilder::success_with_message(
            tag,
            "Tag updated successfully",
        )),
        Err(sqlx::Error::RowNotFound) => Err(StatusCode::NOT_FOUND),
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
    match Tag::delete(state.database(), &id).await {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                Ok(ApiResponseBuilder::success_with_message(
                    (),
                    "Tag deleted successfully",
                ))
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
) -> Result<Json<ApiResponse<Vec<Tag>>>, StatusCode> {
    let tag_query = TagQuery {
        name: query.name,
        tag_type: query.tag_type,
        target_id: None,
        page: query.page,
        page_size: query.page_size,
    };

    match Tag::find_all(state.database(), &tag_query).await {
        Ok(tags) => Ok(ApiResponseBuilder::success(tags)),
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

    match Tag::count(state.database(), &tag_query).await {
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
    // 检查绑定是否已存在
    match TagBinding::exists(state.database(), &request.tag_id, &request.target_id).await {
        Ok(true) => return Err(StatusCode::CONFLICT),
        Ok(false) => {}
        Err(e) => {
            tracing::error!("Failed to check tag binding existence: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    match TagBinding::create(state.database(), &request, &claims.user_id).await {
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
    match TagBinding::delete_by_tag_and_target(state.database(), &query.tag_id, &query.target_id)
        .await
    {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                Ok(ApiResponseBuilder::success_with_message(
                    (),
                    "Tag binding deleted successfully",
                ))
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
        .map(|tag_id| CreateTagBindingRequest {
            tag_id,
            target_id: request.target_id.clone(),
        })
        .collect();

    match TagBinding::create_batch(state.database(), &bindings, &claims.user_id).await {
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
    match TagBinding::delete_all_by_target_id(state.database(), &query.target_id).await {
        Ok(_) => Ok(ApiResponseBuilder::success_with_message(
            (),
            "Tag bindings deleted successfully",
        )),
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
) -> Result<Json<ApiResponse<Vec<Tag>>>, StatusCode> {
    match Tag::find_by_target_id(state.database(), &target_id).await {
        Ok(tags) => Ok(ApiResponseBuilder::success(tags)),
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
) -> Result<Json<ApiResponse<Vec<TagBinding>>>, StatusCode> {
    match TagBinding::find_by_tag_id(state.database(), &tag_id).await {
        Ok(bindings) => Ok(ApiResponseBuilder::success(bindings)),
        Err(e) => {
            tracing::error!("Failed to fetch tag bindings: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
