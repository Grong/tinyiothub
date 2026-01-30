use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;

use crate::{
    api::AppState,
    domain::template::{
        engine::TemplateEngine, repository::TemplateRepository, service::TemplateService,
        validator::TemplateValidator,
    },
    dto::{
        entity::device_template::{
            CreateDeviceTemplateRequest, DeviceCreationInput, DevicePreview, DeviceTemplate,
            TemplateCategory, TemplateQueryParams, UpdateDeviceTemplateRequest,
        },
        response::ApiResponse,
    },
    shared::security::jwt::Claims,
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
        .route("/:id", get(get_template))
        .route("/", post(create_template))
        .route("/:id", put(update_template))
        .route("/:id", delete(delete_template))
        .route("/categories", get(get_template_categories))
        .route("/:id/validate", post(validate_template_input))
        .route("/:id/preview", post(preview_device_from_template))
}

/// 获取模板列表
async fn list_templates(
    State(state): State<AppState>,
    Query(query): Query<TemplateQuery>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<DeviceTemplate>>> {
    // 初始化模板服务
    let template_service = match initialize_template_service(&state).await {
        Ok(service) => service,
        Err(e) => {
            tracing::error!("Failed to initialize template service: {}", e);
            return ApiResponse::error("初始化模板服务失败".to_string());
        }
    };

    let params = TemplateQueryParams {
        category: query.category,
        manufacturer: query.manufacturer,
        device_type: query.device_type,
        protocol_type: query.protocol_type,
        keyword: query.keyword,
        page: query.page,
        page_size: query.page_size,
    };

    match template_service.get_repository().find_all(&params).await {
        Ok(templates) => ApiResponse::success(templates),
        Err(e) => {
            tracing::error!("Failed to list templates: {}", e);
            ApiResponse::error("获取模板列表失败".to_string())
        }
    }
}

/// 获取模板详情
async fn get_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<Option<DeviceTemplate>>> {
    // 初始化模板服务
    let template_service = match initialize_template_service(&state).await {
        Ok(service) => service,
        Err(e) => {
            tracing::error!("Failed to initialize template service: {}", e);
            return ApiResponse::error("初始化模板服务失败".to_string());
        }
    };

    match template_service.get_repository().find_by_id(&id).await {
        Ok(template) => ApiResponse::success(template),
        Err(e) => {
            tracing::error!("Failed to get template {}: {}", id, e);
            ApiResponse::error("获取模板详情失败".to_string())
        }
    }
}

/// 获取模板分类
async fn get_template_categories(
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<TemplateCategory>>> {
    // 初始化模板服务
    let template_service = match initialize_template_service(&state).await {
        Ok(service) => service,
        Err(e) => {
            tracing::error!("Failed to initialize template service: {}", e);
            return ApiResponse::error("初始化模板服务失败".to_string());
        }
    };

    match template_service.get_repository().get_categories().await {
        Ok(categories) => ApiResponse::success(categories),
        Err(e) => {
            tracing::error!("Failed to get template categories: {}", e);
            ApiResponse::error("获取模板分类失败".to_string())
        }
    }
}

/// 创建模板
async fn create_template(
    State(state): State<AppState>,
    _claims: Claims,
    Json(req): Json<CreateDeviceTemplateRequest>,
) -> Json<ApiResponse<DeviceTemplate>> {
    // 初始化模板服务
    let template_service = match initialize_template_service(&state).await {
        Ok(service) => service,
        Err(e) => {
            tracing::error!("Failed to initialize template service: {}", e);
            return ApiResponse::error("初始化模板服务失败".to_string());
        }
    };

    // 验证模板名称唯一性
    match template_service
        .get_repository()
        .exists_by_name(&req.name)
        .await
    {
        Ok(true) => {
            return ApiResponse::error("模板名称已存在".to_string());
        }
        Ok(false) => {
            // 名称可用，继续创建
        }
        Err(e) => {
            tracing::error!("Failed to check template name existence: {}", e);
            return ApiResponse::error("检查模板名称失败".to_string());
        }
    }

    match template_service.get_repository().create(&req).await {
        Ok(created_template) => ApiResponse::success(created_template),
        Err(e) => {
            tracing::error!("Failed to create template: {}", e);
            ApiResponse::error("创建模板失败".to_string())
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
    // 初始化模板服务
    let template_service = match initialize_template_service(&state).await {
        Ok(service) => service,
        Err(e) => {
            tracing::error!("Failed to initialize template service: {}", e);
            return ApiResponse::error("初始化模板服务失败".to_string());
        }
    };

    // 检查模板是否存在
    match template_service.get_repository().find_by_id(&id).await {
        Ok(Some(_template)) => {
            // 模板存在，继续更新
            match template_service.get_repository().update(&id, &req).await {
                Ok(updated_template) => ApiResponse::success(updated_template),
                Err(e) => {
                    tracing::error!("Failed to update template {}: {}", id, e);
                    ApiResponse::error("更新模板失败".to_string())
                }
            }
        }
        Ok(None) => ApiResponse::error("模板不存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to find template {}: {}", id, e);
            ApiResponse::error("查询模板失败".to_string())
        }
    }
}

/// 删除模板
async fn delete_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    // 初始化模板服务
    let template_service = match initialize_template_service(&state).await {
        Ok(service) => service,
        Err(e) => {
            tracing::error!("Failed to initialize template service: {}", e);
            return ApiResponse::error("初始化模板服务失败".to_string());
        }
    };

    match template_service.get_repository().delete(&id).await {
        Ok(deleted) => {
            if deleted {
                tracing::info!("Template {} deleted successfully", id);
                ApiResponse::success(true)
            } else {
                ApiResponse::error("模板不存在".to_string())
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete template {}: {}", id, e);
            ApiResponse::error("删除模板失败".to_string())
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
    // 初始化模板服务
    let template_service = match initialize_template_service(&state).await {
        Ok(service) => service,
        Err(e) => {
            tracing::error!("Failed to initialize template service: {}", e);
            return ApiResponse::error("初始化模板服务失败".to_string());
        }
    };

    // 获取模板
    let template = match template_service.get_repository().find_by_id(&id).await {
        Ok(Some(template)) => template,
        Ok(None) => {
            return ApiResponse::error("模板不存在".to_string());
        }
        Err(e) => {
            tracing::error!("Failed to find template {}: {}", id, e);
            return ApiResponse::error("查询模板失败".to_string());
        }
    };

    // 创建验证器并验证输入
    let validator = TemplateValidator::new();
    let validation_result = validator.validate_user_input(&template, &input);

    // 将验证结果转换为JSON
    match serde_json::to_value(&validation_result) {
        Ok(json_result) => ApiResponse::success(json_result),
        Err(e) => {
            tracing::error!("Failed to serialize validation result: {}", e);
            ApiResponse::error("验证结果序列化失败".to_string())
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
    // 初始化模板服务
    let template_service = match initialize_template_service(&state).await {
        Ok(service) => service,
        Err(e) => {
            tracing::error!("Failed to initialize template service: {}", e);
            return ApiResponse::error("初始化模板服务失败".to_string());
        }
    };

    // 获取模板
    let _template = match template_service.get_repository().find_by_id(&id).await {
        Ok(Some(template)) => template,
        Ok(None) => {
            return ApiResponse::error("模板不存在".to_string());
        }
        Err(e) => {
            tracing::error!("Failed to find template {}: {}", id, e);
            return ApiResponse::error("查询模板失败".to_string());
        }
    };

    // 创建模板引擎并预览设备
    let template_repository = Arc::new(TemplateRepository::new(
        state.database.clone(),
        PathBuf::from("templates"),
    ));
    let validator = Arc::new(TemplateValidator::new());
    let engine = TemplateEngine::new(template_repository, validator);

    match engine.preview_template(&id, &input).await {
        Ok(preview) => ApiResponse::success(preview),
        Err(e) => {
            tracing::error!("Failed to preview device from template {}: {}", id, e);
            ApiResponse::error("预览设备创建失败".to_string())
        }
    }
}

/// 初始化模板服务
async fn initialize_template_service(
    state: &AppState,
) -> Result<TemplateService, Box<dyn std::error::Error + Send + Sync>> {
    let template_repository = Arc::new(TemplateRepository::new(
        state.database.clone(),
        PathBuf::from("templates"),
    ));

    let template_service = TemplateService::new(template_repository);

    // 初始化模板系统（加载内置模板等）
    template_service.initialize().await?;

    Ok(template_service)
}
