use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use tracing::{error, info};

use crate::{
    dto::response::{
        builder::ApiResponseBuilder, ApiResponse, DeviceStatusDistribution, QuickDevice,
    },
    infrastructure::persistence::Database,
    shared::app_state::AppState,
    shared::security::jwt::Claims,
};

#[derive(Debug, Deserialize)]
pub struct QuickDevicesQuery {
    limit: Option<i32>,
}

/// 获取设备状态分布
pub async fn get_device_distribution(
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<DeviceStatusDistribution>> {
    info!("Getting device status distribution");

    let db = Database::new(state.db_pool());

    match get_device_status_distribution(&db).await {
        Ok(distribution) => ApiResponseBuilder::success(distribution),
        Err(e) => {
            error!("Failed to get device status distribution: {}", e);
            ApiResponseBuilder::error("获取设备状态分布失败")
        }
    }
}

/// 获取关键设备列表
pub async fn get_quick_devices(
    State(state): State<AppState>,
    Query(query): Query<QuickDevicesQuery>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<QuickDevice>>> {
    info!("Getting quick devices list with limit: {:?}", query.limit);

    let db = Database::new(state.db_pool());

    let limit = query.limit.unwrap_or(8);
    match get_quick_devices_list(&db, limit).await {
        Ok(devices) => ApiResponseBuilder::success(devices),
        Err(e) => {
            error!("Failed to get quick devices list: {}", e);
            ApiResponseBuilder::error("获取关键设备列表失败")
        }
    }
}

// 辅助函数

/// 获取设备状态分布统计
async fn get_device_status_distribution(
    db: &Database,
) -> Result<DeviceStatusDistribution, sqlx::Error> {
    // 在线设备 (state = 1)
    let online: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state = 1")
        .fetch_one(db.pool())
        .await?;

    // 离线设备 (state = 0)
    let offline: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state = 0")
        .fetch_one(db.pool())
        .await?;

    // 故障设备 (state = -1 或其他错误状态)
    let error: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state < 0")
        .fetch_one(db.pool())
        .await?;

    // 维护中设备 (state = 2)
    let maintenance: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state = 2")
        .fetch_one(db.pool())
        .await?;

    Ok(DeviceStatusDistribution {
        online,
        offline,
        error,
        maintenance,
    })
}

/// 获取关键设备列表
async fn get_quick_devices_list(
    db: &Database,
    limit: i32,
) -> Result<Vec<QuickDevice>, sqlx::Error> {
    let devices =
        sqlx::query_as::<_, (String, String, Option<String>, i32, chrono::NaiveDateTime)>(
            r#"
        SELECT 
            id,
            name,
            device_type,
            state,
            updated_at
        FROM devices 
        ORDER BY 
            CASE 
                WHEN state = 1 THEN 0  -- 在线设备优先
                WHEN state = 0 THEN 1  -- 离线设备其次
                WHEN state < 0 THEN 2  -- 故障设备
                ELSE 3                 -- 其他状态
            END,
            updated_at DESC
        LIMIT ?
        "#,
        )
        .bind(limit)
        .fetch_all(db.pool())
        .await?;

    let quick_devices = devices
        .into_iter()
        .map(|(id, name, device_type, state, updated_at)| {
            let status = match state {
                1 => "online",
                0 => "offline",
                2 => "maintenance",
                _ => "error",
            };

            QuickDevice {
                id,
                name,
                status: status.to_string(),
                last_seen: updated_at.and_utc(),
                device_type: device_type.unwrap_or_else(|| "unknown".to_string()),
            }
        })
        .collect();

    Ok(quick_devices)
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/distribution", get(get_device_distribution))
        .route("/quick", get(get_quick_devices))
}
