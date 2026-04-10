use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use tracing::{error, info};

use crate::{
    api::middleware::WorkspaceScope,
    dto::response::{
        builder::ApiResponseBuilder, ApiResponse, DeviceStatusDistribution, QuickDevice,
    },
    infrastructure::persistence::Database,
    shared::{app_state::AppState, security::jwt::Claims},
};

#[derive(Debug, Deserialize)]
pub struct QuickDevicesQuery {
    limit: Option<i32>,
}

/// 获取设备状态分布
pub async fn get_device_distribution(
    State(state): State<AppState>,
    _claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<DeviceStatusDistribution>> {
    info!("Getting device status distribution");

    let db = Database::new(state.db_pool());

    match get_device_status_distribution(&db, workspace_id.as_deref()).await {
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
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<Vec<QuickDevice>>> {
    info!("Getting quick devices list with limit: {:?}", query.limit);

    let db = Database::new(state.db_pool());

    let limit = query.limit.unwrap_or(8);
    match get_quick_devices_list(&db, limit, workspace_id.as_deref()).await {
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
    workspace_id: Option<&str>,
) -> Result<DeviceStatusDistribution, sqlx::Error> {
    // Use QueryBuilder for each count to avoid lifetime issues with sqlx 0.9
    let online: i64 = if let Some(wid) = workspace_id {
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state = 1 AND workspace_id = ?")
            .bind(wid)
            .fetch_one(db.pool())
            .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state = 1")
            .fetch_one(db.pool())
            .await?
    };

    let offline: i64 = if let Some(wid) = workspace_id {
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state = 0 AND workspace_id = ?")
            .bind(wid)
            .fetch_one(db.pool())
            .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state = 0")
            .fetch_one(db.pool())
            .await?
    };

    let error: i64 = if let Some(wid) = workspace_id {
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state < 0 AND workspace_id = ?")
            .bind(wid)
            .fetch_one(db.pool())
            .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state < 0")
            .fetch_one(db.pool())
            .await?
    };

    let maintenance: i64 = if let Some(wid) = workspace_id {
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state = 2 AND workspace_id = ?")
            .bind(wid)
            .fetch_one(db.pool())
            .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE state = 2")
            .fetch_one(db.pool())
            .await?
    };

    Ok(DeviceStatusDistribution { online, offline, error, maintenance })
}

/// 获取关键设备列表
async fn get_quick_devices_list(
    db: &Database,
    limit: i32,
    workspace_id: Option<&str>,
) -> Result<Vec<QuickDevice>, sqlx::Error> {
    let devices: Vec<(String, String, Option<String>, i32, chrono::NaiveDateTime)> = if let Some(wid) = workspace_id {
        sqlx::query_as(
            r#"
            SELECT id, name, device_type, state, updated_at
            FROM devices WHERE workspace_id = ?
            ORDER BY
                CASE
                    WHEN state = 1 THEN 0
                    WHEN state = 0 THEN 1
                    WHEN state < 0 THEN 2
                    ELSE 3
                END,
                updated_at DESC
            LIMIT ?"#,
        )
        .bind(wid)
        .bind(limit)
        .fetch_all(db.pool())
        .await?
    } else {
        sqlx::query_as(
            r#"
            SELECT id, name, device_type, state, updated_at
            FROM devices
            ORDER BY
                CASE
                    WHEN state = 1 THEN 0
                    WHEN state = 0 THEN 1
                    WHEN state < 0 THEN 2
                    ELSE 3
                END,
                updated_at DESC
            LIMIT ?"#,
        )
        .bind(limit)
        .fetch_all(db.pool())
        .await?
    };

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
