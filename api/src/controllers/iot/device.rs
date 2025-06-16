#![allow(clippy::missing_errors_doc)]
#![allow(clippy::unnecessary_struct_initialization)]
#![allow(clippy::unused_async)]
use crate::services::{
    device_property_service::DevicePropertyService, device_service::DeviceService,
    device_status_service::DeviceStatusService,
};
use axum::extract::Query;
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputDeviceParams {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// 创建设备
pub async fn create_device(
    State(ctx): State<AppContext>,
    Json(params): Json<InputDeviceParams>,
) -> Result<Response> {
    let name = params
        .name
        .ok_or_else(|| Error::BadRequest("name is required".to_string()))?;
    let device =
        DeviceService::create_device(&ctx.db, &name, params.description.as_deref()).await?;

    format::json(device)
}

/// 从模板创建设备
pub async fn create_from_template(
    Path(template_id): Path<i32>,
    State(ctx): State<AppContext>,
    Json(params): Json<InputDeviceParams>,
) -> Result<Response> {
    let name = params
        .name
        .ok_or_else(|| Error::BadRequest("name is required".to_string()))?;
    let device = DeviceService::create_device_from_template(&ctx.db, template_id, &name).await?;

    format::json(device)
}

/// 更新设备属性
pub async fn update_property(
    Path((device_id, property)): Path<(String, String)>,
    State(ctx): State<AppContext>,
    Json(value): Json<Value>,
) -> Result<Response> {
    let property =
        DevicePropertyService::update_property(&ctx.db, &device_id, &property, value).await?;

    format::json(property)
}

/// 获取设备状态
pub async fn get_full_status(
    Path(device_id): Path<String>,
    State(ctx): State<AppContext>,
) -> Result<Response> {
    let status = DeviceStatusService::get_full_status(&ctx.db, &device_id).await?;

    format::json(status)
}

/// 获取设备健康报告
pub async fn get_health_report(
    Path(device_id): Path<String>,
    State(ctx): State<AppContext>,
) -> Result<Response> {
    let health = DeviceStatusService::check_health(&ctx.db, &device_id).await?;

    format::json(health)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryHistoryParams {
    pub hours: Option<i32>,
}

/// 获取属性历史数据
pub async fn get_property_history(
    Path((device_id, property)): Path<(String, String)>,
    State(ctx): State<AppContext>,
    Query(params): Query<QueryHistoryParams>,
) -> Result<Response> {
    let history =
        DeviceStatusService::get_property_history(&ctx.db, &device_id, &property, params.hours)
            .await?;

    format::json(history)
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("iot/api/devices")
        .add("/", post(create_device))
        .add("/{device_id}/properties/{property}", put(update_property))
        .add("/from-template/{template_id}", post(create_from_template))
        .add("/{device_id}/health", get(get_health_report))
        .add("/{device_id}/status", get(get_full_status))
        .add(
            "/{device_id}/properties/{property}/history",
            get(get_property_history),
        )
}
