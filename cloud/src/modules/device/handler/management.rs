use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use serde::Deserialize;
use tinyiothub_core::models::{
    device::{CreateDeviceRequest, Device, DeviceQueryParams, UpdateDeviceRequest},
    template_error::ValidationResult,
};
use tinyiothub_web::response::ApiResponseBuilder;

use crate::{
    api::middleware::WorkspaceScope,
    modules::template::{
        TemplateExporter,
        types::{
            CreateDeviceFromTemplateRequest, DeviceCreationInput, DevicePreview,
            TemplateRequirements,
        },
    },
    shared::{
        api_response::{ApiResponse, PaginatedResponse, PaginationInfo},
        app_state::AppState,
        error_handling::AuthHelper,
        pagination::PaginationQuery,
        security::jwt::Claims,
    },
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateDeviceApiRequest {
    pub name: String,
    pub display_name: Option<String>,
    pub device_type: Option<String>,
    pub address: Option<String>,
    pub description: Option<String>,
    pub position: Option<String>,
    pub driver_name: Option<String>,
    pub device_model: Option<String>,
    pub protocol_type: Option<String>,
    pub factory_name: Option<String>,
    pub linked_data: Option<String>,
    pub connection_config: Option<String>,
    pub parent_id: Option<String>,
    pub product_id: Option<String>,
    #[serde(skip)]
    pub tenant_id: Option<String>, // Set from JWT claims, not from request
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateDeviceApiRequest {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub device_type: Option<String>,
    pub address: Option<String>,
    pub description: Option<String>,
    pub position: Option<String>,
    pub driver_name: Option<String>,
    pub device_model: Option<String>,
    pub protocol_type: Option<String>,
    pub factory_name: Option<String>,
    pub linked_data: Option<String>,
    pub connection_config: Option<String>,
    pub parent_id: Option<String>,
    pub product_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceQuery {
    pub name: Option<String>,
    pub device_type: Option<String>,
    pub driver_name: Option<String>,
    pub state: Option<String>,
    pub product_id: Option<String>,
    pub enabled: Option<bool>,
    pub include_properties: Option<bool>, // 是否包含属性，默认false
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ValidateFieldRequest {
    pub field_name: String,
    pub field_value: String,
}

/// 获取设备列表
async fn list_devices(
    State(state): State<AppState>,
    Query(query): Query<DeviceQuery>,
    claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<PaginatedResponse<Device>>> {
    let params = DeviceQueryParams {
        name: query.name,
        display_name: None,
        device_type: query.device_type,
        address: None,
        driver_name: query.driver_name,
        state: query.state.and_then(|s| s.parse().ok()),
        parent_id: None,
        product_id: query.product_id,
        page: query.pagination.page,
        page_size: query.pagination.page_size,
    };

    let include_properties = query.include_properties.unwrap_or(false);
    let page = query.pagination.page.unwrap_or(1);
    let page_size = query.pagination.page_size.unwrap_or(20);

    // 获取租户感知的设备服务
    let tenant_device_service = state.tenant_device_service(&workspace_id);

    // Get total count for pagination
    let total_count = tenant_device_service.count_devices(&params).await.unwrap_or(0) as u64;
    let total_pages =
        if page_size > 0 { ((total_count as f64) / (page_size as f64)).ceil() as u32 } else { 0 };

    match tenant_device_service.get_devices_with_tags(&params, &claims.tenant_id).await {
        Ok(mut devices) => {
            // 从 DeviceCache 同步实时状态
            for device in &mut devices {
                if let Some(cached_device) = state.device_cache.get(&device.id) {
                    // 始终更新实时状态字段
                    device.status = cached_device.status.clone();
                    device.status = cached_device.status.clone();
                    device.last_heartbeat = cached_device.last_heartbeat.clone();

                    // 根据参数决定是否包含属性
                    if include_properties {
                        device.properties = cached_device.properties.clone();
                    }
                } else if !include_properties {
                    // 如果不需要属性，清空属性字段
                    device.properties = None;
                }
            }

            // 如果不需要属性，确保所有设备都不包含属性
            if !include_properties {
                for device in &mut devices {
                    device.properties = None;
                }
            }

            ApiResponseBuilder::success(PaginatedResponse {
                data: devices,
                pagination: PaginationInfo { page, page_size, total_pages, total_count },
            })
        }
        Err(e) => {
            tracing::error!("Failed to list devices: {}", e);
            ApiResponseBuilder::error("获取设备列表失败")
        }
    }
}

/// 创建设备
async fn create_device(
    State(state): State<AppState>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Json(req): Json<CreateDeviceApiRequest>,
) -> Json<ApiResponse<Device>> {
    let request = CreateDeviceRequest {
        name: req.name,
        display_name: req.display_name,
        device_type: req.device_type,
        address: req.address,
        description: req.description,
        position: req.position,
        driver_name: req.driver_name,
        device_model: req.device_model,
        protocol_type: req.protocol_type,
        factory_name: req.factory_name,
        linked_data: req.linked_data,
        driver_options: req.connection_config,
        parent_id: req.parent_id,
        product_id: req.product_id,
    };

    let tenant_device_service = state.tenant_device_service(&workspace_id);
    match tenant_device_service.create_device(&request).await {
        Ok(created_device) => ApiResponseBuilder::success(created_device),
        Err(e) => {
            tracing::error!("Failed to create device: {}", e);
            match e {
                crate::shared::error::Error::ValidationError(msg) => ApiResponseBuilder::error(msg),
                _ => ApiResponseBuilder::error("创建设备失败"),
            }
        }
    }
}

/// 获取设备详情
async fn get_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<DeviceDetailQuery>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<Option<Device>>> {
    let include_properties = query.include_properties.unwrap_or(true); // 详情默认包含属性

    // Verify device belongs to user's tenant before returning
    let tenant_device_service = state.tenant_device_service(&workspace_id);
    let device_result = tenant_device_service.get_device_by_id(&id).await;

    match device_result {
        Ok(device_opt) => {
            let device = device_opt.map(|mut device| {
                // 从 DeviceCache 同步实时状态
                if let Some(cached_device) = state.device_cache.get(&device.id) {
                    // 始终更新实时状态字段
                    device.status = cached_device.status.clone();
                    device.status = cached_device.status.clone();
                    device.last_heartbeat = cached_device.last_heartbeat.clone();

                    // 根据参数决定是否包含属性和命令
                    if include_properties {
                        device.properties = cached_device.properties.clone();
                        device.commands = cached_device.commands.clone();
                    } else {
                        device.properties = None;
                        device.commands = None;
                    }
                } else if !include_properties {
                    device.properties = None;
                    device.commands = None;
                }
                device
            });

            if device.is_some() {
                ApiResponseBuilder::success(device)
            } else {
                ApiResponseBuilder::error("Device not found".to_string())
            }
        }
        Err(e) => {
            tracing::error!("Failed to get device {}: {}", id, e);
            ApiResponseBuilder::error("获取设备详情失败")
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceDetailQuery {
    pub include_properties: Option<bool>, // 是否包含属性和命令，默认true
}

/// 更新设备
async fn update_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Json(req): Json<UpdateDeviceApiRequest>,
) -> Json<ApiResponse<Device>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters devices by workspace_id

    let update_request = UpdateDeviceRequest {
        name: req.name,
        display_name: req.display_name,
        device_type: req.device_type,
        address: req.address,
        description: req.description,
        position: req.position,
        driver_name: req.driver_name,
        device_model: req.device_model,
        protocol_type: req.protocol_type,
        factory_name: req.factory_name,
        linked_data: req.linked_data,
        driver_options: req.connection_config,
        state: None, // 不在此处更新状态
        parent_id: req.parent_id,
        product_id: req.product_id,
    };

    let tenant_device_service = state.tenant_device_service(&workspace_id);
    match tenant_device_service.update_device(&id, &update_request).await {
        Ok(updated_device) => ApiResponseBuilder::success(updated_device),
        Err(e) => {
            tracing::error!("Failed to update device {}: {}", id, e);
            match e {
                crate::shared::error::Error::NotFound => ApiResponseBuilder::error("设备不存在"),
                crate::shared::error::Error::ValidationError(msg) => ApiResponseBuilder::error(msg),
                _ => ApiResponseBuilder::error("更新设备失败"),
            }
        }
    }
}

/// 删除设备
async fn delete_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<bool>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters devices by workspace_id

    let tenant_device_service = state.tenant_device_service(&workspace_id);
    match tenant_device_service.delete_device(&id).await {
        Ok(success) => {
            if success {
                tracing::info!("Device {} deleted successfully", id);
                ApiResponseBuilder::success(true)
            } else {
                ApiResponseBuilder::error("设备不存在")
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete device {}: {}", id, e);
            match e {
                crate::shared::error::Error::NotFound => ApiResponseBuilder::error("设备不存在"),
                _ => ApiResponseBuilder::error("删除设备失败"),
            }
        }
    }
}

/// 启用设备
async fn enable_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<bool>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters devices by workspace_id

    let tenant_device_service = state.tenant_device_service(&workspace_id);
    match tenant_device_service.update_device_enabled_status(&id, true).await {
        Ok(updated) => {
            if updated {
                tracing::info!("Device {} enabled", id);
                ApiResponseBuilder::success(true)
            } else {
                ApiResponseBuilder::error("设备不存在")
            }
        }
        Err(e) => {
            tracing::error!("Failed to enable device {}: {}", id, e);
            ApiResponseBuilder::error("启用设备失败")
        }
    }
}

/// 禁用设备
async fn disable_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<bool>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters devices by workspace_id

    let tenant_device_service = state.tenant_device_service(&workspace_id);
    match tenant_device_service.update_device_enabled_status(&id, false).await {
        Ok(updated) => {
            if updated {
                tracing::info!("Device {} disabled", id);
                ApiResponseBuilder::success(true)
            } else {
                ApiResponseBuilder::error("设备不存在")
            }
        }
        Err(e) => {
            tracing::error!("Failed to disable device {}: {}", id, e);
            ApiResponseBuilder::error("禁用设备失败")
        }
    }
}

/// 基于模板创建设备 (需求 4.5, 3.6)
async fn create_device_from_template(
    State(state): State<AppState>,
    claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Json(req): Json<CreateDeviceFromTemplateRequest>,
) -> Json<ApiResponse<Device>> {
    // Set tenant_id from authenticated user's claims
    let mut device_input = req.device_input;
    device_input.tenant_id = Some(claims.tenant_id);
    device_input.workspace_id = workspace_id.clone();

    let tenant_device_service = state.tenant_device_service(&workspace_id);
    match tenant_device_service
        .create_device_from_template(state.template_engine(), &req.template_id, &device_input)
        .await
    {
        Ok(device) => ApiResponseBuilder::success(device),
        Err(e) => {
            tracing::error!("Failed to create device from template: {}", e);
            ApiResponseBuilder::error(format!("创建设备失败: {}", e))
        }
    }
}

/// 预览基于模板的设备创建 (需求 3.4)
async fn preview_device_from_template(
    State(state): State<AppState>,
    Path(template_id): Path<String>,
    _claims: Claims,
    Json(device_input): Json<DeviceCreationInput>,
) -> Json<ApiResponse<DevicePreview>> {
    tracing::info!(
        "Previewing device from template: template_id={}, device_name={}",
        template_id,
        device_input.name
    );

    match state.template_engine().preview_template(&template_id, &device_input).await {
        Ok(preview) => {
            tracing::debug!(
                "Device preview generated: properties={}, commands={}, warnings={}",
                preview.properties.len(),
                preview.commands.len(),
                preview.warnings.len()
            );
            ApiResponseBuilder::success(preview)
        }
        Err(e) => {
            tracing::error!("Failed to preview device from template {}: {}", template_id, e);
            ApiResponseBuilder::error(format!("设备预览失败: {}", e))
        }
    }
}

/// 验证基于模板的设备输入 (需求 3.7, 6.2, 6.3, 6.4)
async fn validate_device_input(
    State(state): State<AppState>,
    Path(template_id): Path<String>,
    _claims: Claims,
    Json(device_input): Json<DeviceCreationInput>,
) -> Json<ApiResponse<ValidationResult>> {
    tracing::info!(
        "Validating device input for template: template_id={}, device_name={}",
        template_id,
        device_input.name
    );

    match state.template_engine().validate_user_input(&template_id, &device_input).await {
        Ok(validation_result) => {
            tracing::debug!(
                "Device input validation completed: errors={}, warnings={}",
                validation_result.errors.len(),
                validation_result.warnings.len()
            );
            ApiResponseBuilder::success(validation_result)
        }
        Err(e) => {
            tracing::error!("Failed to validate device input for template {}: {}", template_id, e);
            ApiResponseBuilder::error(format!("输入验证失败: {}", e))
        }
    }
}

/// 获取模板需求信息 (用于设备创建向导) (需求 3.5)
async fn get_template_requirements(
    State(state): State<AppState>,
    Path(template_id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<TemplateRequirements>> {
    tracing::info!("Getting template requirements for wizard: template_id={}", template_id);

    match state.template_engine().get_template_requirements(&template_id).await {
        Ok(requirements) => {
            tracing::debug!(
                "Template requirements retrieved: required_fields={}, properties={}, commands={}",
                requirements.required_fields.len(),
                requirements.available_properties.len(),
                requirements.available_commands.len()
            );
            ApiResponseBuilder::success(requirements)
        }
        Err(e) => {
            tracing::error!("Failed to get template requirements for {}: {}", template_id, e);
            ApiResponseBuilder::error(format!("获取模板需求失败: {}", e))
        }
    }
}

/// 验证单个字段 (用于设备创建向导的实时验证) (需求 3.7)
async fn validate_single_field(
    State(state): State<AppState>,
    Path(template_id): Path<String>,
    _claims: Claims,
    Json(request): Json<ValidateFieldRequest>,
) -> Json<ApiResponse<ValidationResult>> {
    tracing::debug!(
        "Validating single field for template: template_id={}, field={}, value={}",
        template_id,
        request.field_name,
        request.field_value
    );

    match state
        .template_engine()
        .validate_field(&template_id, &request.field_name, &request.field_value)
        .await
    {
        Ok(validation_result) => {
            tracing::debug!(
                "Single field validation completed: field={}, errors={}, warnings={}",
                request.field_name,
                validation_result.errors.len(),
                validation_result.warnings.len()
            );
            ApiResponseBuilder::success(validation_result)
        }
        Err(e) => {
            tracing::error!(
                "Failed to validate field {} for template {}: {}",
                request.field_name,
                template_id,
                e
            );
            ApiResponseBuilder::error(format!("字段验证失败: {}", e))
        }
    }
}

/// Export a device as a template
async fn export_device_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<serde_json::Value>> {
    match AuthHelper::check_role(&state, &claims.user_id, "admin").await {
        Ok(true) => {}
        Ok(false) => return ApiResponseBuilder::error("需要管理员权限"),
        Err(e) => {
            tracing::warn!("权限检查失败: {}", e);
            return ApiResponseBuilder::error("权限检查失败");
        }
    }

    let tenant_device_service = state.tenant_device_service(&workspace_id);
    match tenant_device_service.get_device_by_id(&id).await {
        Ok(Some(device)) => {
            match TemplateExporter::export_from_device(&device) {
                Ok(template) => {
                    // Save the template to the database
                    let display_name: std::collections::HashMap<String, String> =
                        serde_json::from_str(&template.display_name).unwrap_or_default();
                    let description = template.description.as_ref().map(|d| {
                        let mut map = std::collections::HashMap::new();
                        map.insert("zh".to_string(), d.clone());
                        map.insert("en".to_string(), d.clone());
                        map
                    });
                    let create_req = crate::modules::template::types::CreateDeviceTemplateRequest {
                        name: template.name.clone(),
                        display_name,
                        description,
                        version: template.version.clone(),
                        author: template.author.clone(),
                        category: template.category.clone(),
                        manufacturer: template.manufacturer.clone(),
                        device_type: template.device_type.clone(),
                        protocol_type: template.protocol_type.clone(),
                        driver_name: template.driver_name.clone(),
                        tags: template.get_tags(),
                        device_info: template.get_device_info().unwrap_or(
                            crate::modules::template::types::DeviceInfo {
                                default_name_pattern: format!("{}_{{index}}", device.name),
                                default_display_name_pattern: None,
                                default_description: None,
                                default_position: device.position.clone(),
                                default_driver_options: None,
                                required_fields: vec!["name".to_string()],
                            },
                        ),
                        properties: template.get_properties().unwrap_or_default(),
                        commands: template.get_commands().unwrap_or_default(),
                        workspace_id: workspace_id.clone(),
                    };

                    match crate::modules::template::types::DeviceTemplate::create(
                        &state.database,
                        &create_req,
                    )
                    .await
                    {
                        Ok(saved) => ApiResponseBuilder::success(serde_json::json!({
                            "template_id": saved.id,
                            "name": saved.name,
                        })),
                        Err(e) => {
                            tracing::error!("Failed to save exported template: {}", e);
                            ApiResponseBuilder::error(format!("Failed to save template: {}", e))
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to export device {} as template: {}", id, e);
                    ApiResponseBuilder::error(format!("Export failed: {}", e))
                }
            }
        }
        Ok(None) => ApiResponseBuilder::error("Device not found"),
        Err(e) => {
            tracing::error!("Failed to get device {}: {}", id, e);
            ApiResponseBuilder::error("Failed to get device")
        }
    }
}

/// Clone an existing device
async fn clone_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
    claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<Device>> {
    match AuthHelper::check_role(&state, &claims.user_id, "admin").await {
        Ok(true) => {}
        Ok(false) => return ApiResponseBuilder::error("需要管理员权限"),
        Err(e) => {
            tracing::warn!("权限检查失败: {}", e);
            return ApiResponseBuilder::error("权限检查失败");
        }
    }

    let tenant_device_service = state.tenant_device_service(&workspace_id);

    // Get the source device
    let source_device = match tenant_device_service.get_device_by_id(&id).await {
        Ok(Some(d)) => d,
        Ok(None) => return ApiResponseBuilder::error("Device not found"),
        Err(e) => {
            tracing::error!("Failed to get device {}: {}", id, e);
            return ApiResponseBuilder::error("Failed to get device");
        }
    };

    // Create a new device based on the source
    let new_name = format!("{}_copy", source_device.name);
    let request = CreateDeviceRequest {
        name: new_name,
        display_name: source_device.display_name.clone(),
        device_type: source_device.device_type.clone(),
        address: source_device.address.clone(),
        description: source_device.description.clone(),
        position: source_device.position.clone(),
        driver_name: source_device.driver_name.clone(),
        device_model: source_device.device_model.clone(),
        protocol_type: source_device.protocol_type.clone(),
        factory_name: source_device.factory_name.clone(),
        linked_data: source_device.linked_data.clone(),
        driver_options: source_device.driver_options.clone(),
        parent_id: source_device.parent_id.clone(),
        product_id: source_device.product_id.clone(),
    };

    match tenant_device_service.create_device(&request).await {
        Ok(created_device) => ApiResponseBuilder::success(created_device),
        Err(e) => {
            tracing::error!("Failed to clone device {}: {}", id, e);
            match e {
                crate::shared::error::Error::ValidationError(msg) => ApiResponseBuilder::error(msg),
                _ => ApiResponseBuilder::error("Clone device failed"),
            }
        }
    }
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_devices).post(create_device))
        .route("/{id}", get(get_device).put(update_device).delete(delete_device))
        .route("/{id}/enable", post(enable_device))
        .route("/{id}/disable", post(disable_device))
        .route("/{id}/export-template", post(export_device_template))
        .route("/{id}/clone", post(clone_device))
        .route("/from-template", post(create_device_from_template))
        .route("/from-template/{template_id}/preview", post(preview_device_from_template))
        .route("/from-template/{template_id}/validate", post(validate_device_input))
        .route("/from-template/{template_id}/requirements", get(get_template_requirements))
        .route("/from-template/{template_id}/validate-field", post(validate_single_field))
}
