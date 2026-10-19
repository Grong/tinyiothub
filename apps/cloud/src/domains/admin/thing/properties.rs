use crate::domains::admin::AdminState;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use serde::Deserialize;
use tinyiothub_core::models::thing_property::{CreateThingPropertyRequest, DerivedStatusInfo, ThingProperty};
use tinyiothub_web::response::ApiResponseBuilder;
use tinyiothub_web::security::Claims;

use crate::domains::thing::service::derived_validation;
use tinyiothub_web::api_response::ApiResponse;
use tinyiothub_web::middleware::workspace::WorkspaceScope;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdatePropertyValueRequest {
    pub value: String,
}

/// 创建属性定义（spec §四）：带 expression 即派生。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreatePropertyRequest {
    pub name: String,
    pub display_name: Option<String>,
    pub unit: Option<String>,
    pub data_type: Option<String>,
    pub expression: Option<String>,
}

/// 更新属性定义（PUT = 替换语义）：expression 给出则全量重新校验并写穿派生
/// 三件套；缺省则保留既有派生定义（不做 derived → 非派生转换）。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdatePropertyRequest {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub unit: Option<String>,
    pub data_type: Option<String>,
    pub expression: Option<String>,
}

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AdminState: axum::extract::FromRef<S>,
    std::sync::Arc<tinyiothub_authn::jwt::JwtService>: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/{thing_id}/properties", get(get_thing_properties))
        .route("/{thing_id}/properties", post(create_property))
        .route("/{thing_id}/properties/{property_id}", put(update_property))
        .route("/{thing_id}/properties/{property_id}", delete(delete_property))
        .route("/{thing_id}/properties/{property_id}/value", put(update_property_value))
        .route(
            "/by-name/{device_name}/properties/{property_name}",
            get(get_device_property_by_name),
        )
}

fn err<T>(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ApiResponse<T>>) {
    (status, ApiResponseBuilder::error_with_code(status.as_u16() as i32, msg))
}

/// UNIQUE(thing_id, name) 冲突判定（创建/更新共用 → 409）。
fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(d) if d.is_unique_violation())
}

/// 派生定义校验 + refs JSON 序列化（创建/更新共用）。
/// Err 文本直接作为 400 body（语法定位/无效引用/环路径，前端行内红字）。
async fn validate_derived(
    state: &AdminState,
    workspace_id: &str,
    thing_id: &str,
    expression: &str,
) -> Result<String, (StatusCode, Json<ApiResponse<ThingProperty>>)> {
    let resolved =
        derived_validation::validate_derived_expression(&state.db_pool(), workspace_id, thing_id, expression)
            .await
            .map_err(|msg| err(StatusCode::BAD_REQUEST, msg))?;
    let refs: Vec<serde_json::Value> = resolved
        .iter()
        .map(|r| serde_json::json!({"thing_id": r.thing_id, "property_name": r.property_name}))
        .collect();
    // refs 恒写非空 JSON（无引用写 '[]'——descendant_count-only 表达式合法，Task 6 评审）
    Ok(serde_json::to_string(&refs).unwrap_or_else(|_| "[]".to_string()))
}

/// 物归属校验（workspace 作用域，D3）：不存在 → 404。
async fn require_thing<T>(
    state: &AdminState,
    workspace_id: &Option<String>,
    thing_id: &str,
) -> Result<(), (StatusCode, Json<ApiResponse<T>>)> {
    let tenant_device_service = state.tenant_device_service(workspace_id);
    match tenant_device_service.get_thing_by_id(thing_id).await {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(err(StatusCode::NOT_FOUND, "物不存在")),
        Err(e) => {
            tracing::error!("Failed to load thing {}: {}", thing_id, e);
            Err(err(StatusCode::INTERNAL_SERVER_ERROR, "查询物失败"))
        }
    }
}

/// Task 7 评审 ⚠️ 钩子①：物管理面配置增删改成功路径必须通知 Manager
/// （漏接 = 索引/驱动静默失效）。Manager 未装配（测试/降级）时跳过。
async fn notify_config_changed(state: &AdminState, thing_id: &str) {
    if let Some(m) = &state.derived_manager {
        m.notify_config_changed(thing_id).await;
    }
}

/// 获取设备属性列表（派生行注入运行状态，spec §四）
async fn get_thing_properties(
    State(state): State<AdminState>,
    Path(thing_id): Path<String>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<Vec<ThingProperty>>> {
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters things by workspace_id

    let tenant_device_service = state.tenant_device_service(&workspace_id);
    match tenant_device_service.get_thing_properties(&thing_id).await {
        Ok(mut properties) => {
            // 派生行注入运行状态（内存态不落库，spec §二 E4）
            if let Some(m) = &state.derived_manager {
                for p in properties.iter_mut().filter(|p| p.source.as_deref() == Some("derived")) {
                    p.derived_status = m.derived_status(&p.id).map(|s| DerivedStatusInfo {
                        stale: s.stale,
                        stale_reason: s.stale_reason,
                        last_eval_at: s.last_eval_at,
                    });
                }
            }
            ApiResponseBuilder::success(properties)
        }
        Err(e) => {
            tracing::error!("Failed to get device properties for {}: {}", thing_id, e);
            ApiResponseBuilder::error("获取设备属性失败")
        }
    }
}

/// 创建属性定义：带 expression 即派生（spec §四）
async fn create_property(
    State(state): State<AdminState>,
    Path(thing_id): Path<String>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Json(req): Json<CreatePropertyRequest>,
) -> (StatusCode, Json<ApiResponse<ThingProperty>>) {
    if req.name.trim().is_empty() {
        return err(StatusCode::BAD_REQUEST, "属性名不能为空");
    }
    if let Err(resp) = require_thing(&state, &workspace_id, &thing_id).await {
        return resp;
    }
    let ws = workspace_id.clone().unwrap_or_default();

    let (source, refs_json, data_type, is_read_only) = match &req.expression {
        Some(expr) => {
            let refs = match validate_derived(&state, &ws, &thing_id, expr).await {
                Ok(r) => r,
                Err(resp) => return resp,
            };
            // 派生：source 显式 'derived'（Task 4 评审），data_type 缺省 'number'，只读
            (
                Some("derived".to_string()),
                Some(refs),
                Some(req.data_type.clone().unwrap_or_else(|| "number".to_string())),
                Some(1),
            )
        }
        // data_type 列 NOT NULL：缺省补 'string'（与列默认一致，INSERT 显式绑定
        // 不吃表默认）
        None => (
            None,
            None,
            Some(req.data_type.clone().unwrap_or_else(|| "string".to_string())),
            None,
        ),
    };

    let created = match state
        .db()
        .create_thing_properties_batch(&[CreateThingPropertyRequest {
            thing_id: thing_id.clone(),
            name: req.name.clone(),
            display_name: req.display_name.clone(),
            description: None,
            data_type,
            unit: req.unit.clone(),
            min_value: None,
            max_value: None,
            default_value: None,
            is_read_only,
            expression: req.expression.clone(),
            source,
            refs: refs_json,
        }])
        .await
    {
        Ok(mut v) if !v.is_empty() => v.remove(0),
        Ok(_) => return err(StatusCode::INTERNAL_SERVER_ERROR, "创建属性失败"),
        Err(e) if is_unique_violation(&e) => {
            return err(StatusCode::CONFLICT, format!("属性 {} 已存在", req.name));
        }
        Err(e) => {
            tracing::error!("Failed to create property for {}: {}", thing_id, e);
            return err(StatusCode::INTERNAL_SERVER_ERROR, "创建属性失败");
        }
    };

    notify_config_changed(&state, &thing_id).await;
    (StatusCode::OK, ApiResponseBuilder::success(created))
}

/// 更新属性定义（PUT = 替换语义；改 expression → 全量重新校验）
async fn update_property(
    State(state): State<AdminState>,
    Path((thing_id, property_id)): Path<(String, String)>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
    Json(req): Json<UpdatePropertyRequest>,
) -> (StatusCode, Json<ApiResponse<ThingProperty>>) {
    if let Err(resp) = require_thing(&state, &workspace_id, &thing_id).await {
        return resp;
    }
    let existing = match state.db().find_thing_property_by_id(&property_id).await {
        Ok(Some(p)) if p.thing_id == thing_id => p,
        Ok(_) => return err(StatusCode::NOT_FOUND, "属性不存在"),
        Err(e) => {
            tracing::error!("Failed to load property {}: {}", property_id, e);
            return err(StatusCode::INTERNAL_SERVER_ERROR, "查询属性失败");
        }
    };
    let ws = workspace_id.clone().unwrap_or_default();

    // expression 给出 → 全量重新校验（PUT 语义）。环检测把该属性旧边与新边合并
    // 判定：库内既存图无环时，合并图成环 ⟺ 新边成环（旧边参与的回边在旧图中
    // 已构成环，与既存无环不变量矛盾），故合并判定无假阳性，无需先摘旧边。
    let derived = match &req.expression {
        Some(expr) => match validate_derived(&state, &ws, &thing_id, expr).await {
            Ok(refs) => Some((expr.as_str(), refs)),
            Err(resp) => return resp,
        },
        None => None,
    };

    let affected = match state
        .db()
        .update_thing_property_definition(
            &property_id,
            &thing_id,
            req.name.as_deref(),
            req.display_name.as_deref(),
            req.unit.as_deref(),
            req.data_type.as_deref(),
            derived.as_ref().map(|(e, r)| (*e, r.as_str())),
        )
        .await
    {
        Ok(n) => n,
        Err(e) if is_unique_violation(&e) => {
            return err(
                StatusCode::CONFLICT,
                format!("属性名冲突: {}", req.name.as_deref().unwrap_or(&existing.name)),
            );
        }
        Err(e) => {
            tracing::error!("Failed to update property {}: {}", property_id, e);
            return err(StatusCode::INTERNAL_SERVER_ERROR, "更新属性失败");
        }
    };
    if affected == 0 {
        return err(StatusCode::NOT_FOUND, "属性不存在");
    }

    notify_config_changed(&state, &thing_id).await;
    match state.db().find_thing_property_by_id(&property_id).await {
        Ok(Some(p)) => (StatusCode::OK, ApiResponseBuilder::success(p)),
        _ => err(StatusCode::INTERNAL_SERVER_ERROR, "回读属性失败"),
    }
}

/// 删除属性定义（最后一个派生删除时 Manager 经 notify 回收驱动 defs）
async fn delete_property(
    State(state): State<AdminState>,
    Path((thing_id, property_id)): Path<(String, String)>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> (StatusCode, Json<ApiResponse<bool>>) {
    if let Err(resp) = require_thing::<bool>(&state, &workspace_id, &thing_id).await {
        return resp;
    }
    match state.db().delete_thing_property(&property_id, &thing_id).await {
        Ok(0) => err(StatusCode::NOT_FOUND, "属性不存在"),
        Ok(_) => {
            notify_config_changed(&state, &thing_id).await;
            (StatusCode::OK, ApiResponseBuilder::success(true))
        }
        Err(e) => {
            tracing::error!("Failed to delete property {}: {}", property_id, e);
            err(StatusCode::INTERNAL_SERVER_ERROR, "删除属性失败")
        }
    }
}

/// 通过设备名称和属性名称获取属性
async fn get_device_property_by_name(
    State(state): State<AdminState>,
    Path((device_name, property_name)): Path<(String, String)>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<Option<ThingProperty>>> {
    // 先通过名称查找设备，再验证租户
    let tenant_device_service = state.tenant_device_service(&workspace_id);
    let _device = match tenant_device_service.get_device_by_name(&device_name).await {
        Ok(Some(d)) => d,
        Ok(None) => return ApiResponseBuilder::error("设备不存在"),
        Err(e) => {
            tracing::error!("Failed to find device by name {}: {}", device_name, e);
            return ApiResponseBuilder::error("查询设备失败");
        }
    };
    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters things by workspace_id
    let property = state.get_device_prop_by_name(&device_name, &property_name);
    ApiResponseBuilder::success(property)
}

/// 更新设备属性值
async fn update_property_value(
    State(state): State<AdminState>,
    Path((thing_id, property_id)): Path<(String, String)>,
    claims: Claims,
    Json(req): Json<UpdatePropertyValueRequest>,
) -> (StatusCode, Json<ApiResponse<bool>>) {
    // T3：派生属性值只由 DerivedDriver 写（spec §四）——先读属性再判
    match state.db().find_thing_property_by_id(&property_id).await {
        Ok(Some(p)) if p.thing_id == thing_id && p.source.as_deref() == Some("derived") => {
            return err(StatusCode::BAD_REQUEST, "派生属性不支持手动写值");
        }
        _ => {}
    }

    // Note: Tenant verification is now handled by the TenantDeviceRepository adapter
    // which automatically filters things by workspace_id
    match state
        .update_device_property_value(&claims.workspace_id, &thing_id, &property_id, &req.value)
        .await
    {
        Ok(()) => {
            tracing::info!(
                "Property value updated: device={}, property={}, value={}",
                thing_id,
                property_id,
                req.value
            );
            (StatusCode::OK, ApiResponseBuilder::success(true))
        }
        Err(e) => {
            tracing::error!("Failed to update property value: {}", e);
            match e {
                tinyiothub_core::error::Error::NotFound => {
                    (StatusCode::OK, ApiResponseBuilder::error("设备或属性不存在"))
                }
                tinyiothub_core::error::Error::ValidationError(msg) => (StatusCode::OK, ApiResponseBuilder::error(msg)),
                _ => (StatusCode::OK, ApiResponseBuilder::error("更新属性值失败")),
            }
        }
    }
}
