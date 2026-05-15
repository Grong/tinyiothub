use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
};
use serde::Deserialize;
use tinyiothub_web::response::ApiResponseBuilder;

use crate::{
    modules::template::{
        service::{TemplateService, TemplateValidator},
        types::{
            CreateDeviceTemplateRequest, DeviceCreationInput, DevicePreview, DeviceTemplate,
            TemplateCategory, TemplateQueryParams, UpdateDeviceTemplateRequest,
        },
    },
    shared::{
        api_response::{ApiResponse, PaginatedResponse, PaginationInfo},
        app_state::AppState,
        security::jwt::Claims,
    },
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TemplateQuery {
    pub category: Option<String>,
    pub manufacturer: Option<String>,
    pub device_type: Option<String>,
    pub protocol_type: Option<String>,
    pub keyword: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_templates))
        .route("/{id}", get(get_template))
        .route("/", post(create_template))
        .route("/{id}", put(update_template))
        .route("/{id}", delete(delete_template))
        .route("/categories", get(get_template_categories))
        .route("/{id}/validate", post(validate_template_input))
        .route("/{id}/preview", post(preview_device_from_template))
}

/// 获取模板列表
async fn list_templates(
    State(state): State<AppState>,
    Query(query): Query<TemplateQuery>,
    _claims: Claims,
) -> Json<ApiResponse<PaginatedResponse<DeviceTemplate>>> {
    // 初始化模板服务
    let template_service = get_template_service(&state);

    let params = TemplateQueryParams {
        category: query.category,
        manufacturer: query.manufacturer,
        device_type: query.device_type,
        protocol_type: query.protocol_type,
        keyword: query.keyword,
        page: query.page,
        page_size: query.page_size,
    };

    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(20);

    let (templates_result, total_result) = tokio::join!(
        template_service.get_repository().find_all(&params),
        template_service.get_repository().count(&params),
    );

    match templates_result {
        Ok(templates) => {
            let total = total_result.unwrap_or(0) as u64;
            let total_count = total;
            let total_pages = if page_size > 0 {
                ((total_count as f64) / (page_size as f64)).ceil() as u32
            } else {
                0
            };

            ApiResponseBuilder::success(PaginatedResponse {
                data: templates,
                pagination: PaginationInfo { page, page_size, total_pages, total_count },
            })
        }
        Err(e) => {
            tracing::error!("Failed to list templates: {}", e);
            ApiResponseBuilder::error("获取模板列表失败".to_string())
        }
    }
}

/// 获取模板详情
async fn get_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<Option<DeviceTemplate>>> {
    let template_service = get_template_service(&state);

    match template_service.get_repository().find_by_id(&id).await {
        Ok(template) => ApiResponseBuilder::success(template),
        Err(e) => {
            tracing::error!("Failed to get template {}: {}", id, e);
            ApiResponseBuilder::error("获取模板详情失败".to_string())
        }
    }
}

/// 获取模板分类
async fn get_template_categories(
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<TemplateCategory>>> {
    let template_service = get_template_service(&state);

    match template_service.get_repository().get_categories().await {
        Ok(categories) => ApiResponseBuilder::success(categories),
        Err(e) => {
            tracing::error!("Failed to get template categories: {}", e);
            ApiResponseBuilder::error("获取模板分类失败".to_string())
        }
    }
}

/// 创建模板
async fn create_template(
    State(state): State<AppState>,
    _claims: Claims,
    Json(req): Json<CreateDeviceTemplateRequest>,
) -> Json<ApiResponse<DeviceTemplate>> {
    let template_service = get_template_service(&state);

    // 验证模板名称唯一性
    match template_service.get_repository().exists_by_name(&req.name).await {
        Ok(true) => {
            return ApiResponseBuilder::error("模板名称已存在".to_string());
        }
        Ok(false) => {
            // 名称可用，继续创建
        }
        Err(e) => {
            tracing::error!("Failed to check template name existence: {}", e);
            return ApiResponseBuilder::error("检查模板名称失败".to_string());
        }
    }

    match template_service.get_repository().create(&req).await {
        Ok(created_template) => ApiResponseBuilder::success(created_template),
        Err(e) => {
            tracing::error!("Failed to create template: {}", e);
            ApiResponseBuilder::error("创建模板失败".to_string())
        }
    }
}

/// 更新模板
async fn update_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
    Json(req): Json<UpdateDeviceTemplateRequest>,
) -> Json<ApiResponse<DeviceTemplate>> {
    let template_service = get_template_service(&state);

    // 检查模板是否存在
    match template_service.get_repository().find_by_id(&id).await {
        Ok(Some(_template)) => match template_service.get_repository().update(&id, &req).await {
            Ok(updated_template) => ApiResponseBuilder::success(updated_template),
            Err(e) => {
                tracing::error!("Failed to update template {}: {}", id, e);
                ApiResponseBuilder::error("更新模板失败".to_string())
            }
        },
        Ok(None) => ApiResponseBuilder::error("模板不存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to find template {}: {}", id, e);
            ApiResponseBuilder::error("查询模板失败".to_string())
        }
    }
}

/// 删除模板
async fn delete_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    let template_service = get_template_service(&state);

    match template_service.get_repository().delete(&id).await {
        Ok(deleted) => {
            if deleted {
                tracing::info!("Template {} deleted successfully", id);
                ApiResponseBuilder::success(true)
            } else {
                ApiResponseBuilder::error("模板不存在".to_string())
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete template {}: {}", id, e);
            ApiResponseBuilder::error("删除模板失败".to_string())
        }
    }
}

/// 验证用户输入
async fn validate_template_input(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
    Json(input): Json<DeviceCreationInput>,
) -> Json<ApiResponse<serde_json::Value>> {
    let template_service = get_template_service(&state);

    // 获取模板
    let template = match template_service.get_repository().find_by_id(&id).await {
        Ok(Some(template)) => template,
        Ok(None) => {
            return ApiResponseBuilder::error("模板不存在".to_string());
        }
        Err(e) => {
            tracing::error!("Failed to find template {}: {}", id, e);
            return ApiResponseBuilder::error("查询模板失败".to_string());
        }
    };

    // 创建验证器并验证输入
    let validator = TemplateValidator::new();
    let validation_result = validator.validate_user_input(&template, &input);

    // 将验证结果转换为JSON
    match serde_json::to_value(&validation_result) {
        Ok(json_result) => ApiResponseBuilder::success(json_result),
        Err(e) => {
            tracing::error!("Failed to serialize validation result: {}", e);
            ApiResponseBuilder::error("验证结果序列化失败".to_string())
        }
    }
}

/// 预览基于模板的设备创建
async fn preview_device_from_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
    Json(input): Json<DeviceCreationInput>,
) -> Json<ApiResponse<DevicePreview>> {
    let template_service = get_template_service(&state);

    // 获取模板
    let _template = match template_service.get_repository().find_by_id(&id).await {
        Ok(Some(template)) => template,
        Ok(None) => {
            return ApiResponseBuilder::error("模板不存在".to_string());
        }
        Err(e) => {
            tracing::error!("Failed to find template {}: {}", id, e);
            return ApiResponseBuilder::error("查询模板失败".to_string());
        }
    };

    // 使用已初始化的模板引擎预览设备
    let engine = state.template_engine();

    match engine.preview_template(&id, &input).await {
        Ok(preview) => ApiResponseBuilder::success(preview),
        Err(e) => {
            tracing::error!("Failed to preview device from template {}: {}", id, e);
            ApiResponseBuilder::error("预览设备创建失败".to_string())
        }
    }
}

/// 初始化模板服务
fn get_template_service(state: &AppState) -> TemplateService {
    TemplateService::new(state.template_engine().get_repository_arc())
}
