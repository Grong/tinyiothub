use axum::{extract::State, response::Json, routing::get, Router};
use serde::Deserialize;
use tracing::info;

use crate::{
    api::{middleware::WorkspaceScope, AppState},
    dto::response::{ApiResponse, DashboardMetrics, DashboardStats, MonthlyGrowth, NetworkMetrics},
    infrastructure::persistence::Database,
    shared::security::jwt::Claims,
};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct TrendQuery {
    period: Option<String>, // "24h", "7d", "30d"
}

/// 获取 Dashboard 统计信息
/// GET /api/monitoring/stats
pub async fn get_dashboard_stats(
    State(state): State<AppState>,
    claims: Claims,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<DashboardStats>> {
    info!("获取 Dashboard 统计信息, 用户: {}", claims.username);

    let db = Database::new(state.db_pool());

    // 获取设备统计
    let total_devices = get_total_devices_count(&db, workspace_id.as_deref()).await.unwrap_or(0);
    let online_devices = get_online_devices_count(&db, workspace_id.as_deref()).await.unwrap_or(0);

    // 获取告警统计
    let active_alarms = get_active_alarms_count(&db, workspace_id.as_deref()).await.unwrap_or(0);

    // 获取系统状态
    let system_status = determine_system_status(online_devices, total_devices, active_alarms);

    // 获取系统运行时间（模拟数据）
    let system_uptime = get_system_uptime().await.unwrap_or(0);

    // 获取今日消息数（模拟数据）
    let today_messages = get_today_messages_count(&db).await.unwrap_or(0);

    // 获取月度增长数据
    let monthly_growth =
        get_monthly_growth(&db).await.unwrap_or(MonthlyGrowth { devices: 0, messages: 0 });

    let stats = DashboardStats {
        total_devices,
        online_devices,
        active_alarms,
        system_status,
        system_uptime,
        today_messages,
        monthly_growth,
    };

    ApiResponse::success(stats)
}

/// 获取系统性能指标
/// GET /api/monitoring/metrics
pub async fn get_dashboard_metrics(
    State(_state): State<AppState>,
    claims: Claims,
) -> Json<ApiResponse<DashboardMetrics>> {
    info!("获取系统性能指标, 用户: {}", claims.username);

    // 获取系统性能指标（这里使用模拟数据，实际项目中应该从系统监控服务获取）
    let metrics = DashboardMetrics {
        cpu: get_cpu_usage().await.unwrap_or(25.5),
        memory: get_memory_usage().await.unwrap_or(68.2),
        disk: get_disk_usage().await.unwrap_or(45.8),
        network: NetworkMetrics {
            inbound: get_network_inbound().await.unwrap_or(1024 * 1024 * 50), // 50MB
            outbound: get_network_outbound().await.unwrap_or(1024 * 1024 * 30), // 30MB
        },
    };

    ApiResponse::success(metrics)
}

// 辅助函数

/// 获取设备总数
async fn get_total_devices_count(db: &Database, workspace_id: Option<&str>) -> Result<i64, sqlx::Error> {
    let (query_str, wid) = match workspace_id {
        Some(wid) => ("SELECT COUNT(*) FROM devices WHERE workspace_id = ?", Some(wid)),
        None => ("SELECT COUNT(*) FROM devices", None),
    };
    let mut q = sqlx::query_scalar(sqlx::AssertSqlSafe(query_str));
    if let Some(w) = wid { q = q.bind(w); }
    let count: i64 = q.fetch_one(db.pool()).await?;
    Ok(count)
}

/// 获取在线设备数
async fn get_online_devices_count(db: &Database, workspace_id: Option<&str>) -> Result<i64, sqlx::Error> {
    let (query_str, wid) = match workspace_id {
        Some(wid) => ("SELECT COUNT(*) FROM devices WHERE state = 1 AND workspace_id = ?", Some(wid)),
        None => ("SELECT COUNT(*) FROM devices WHERE state = 1", None),
    };
    let mut q = sqlx::query_scalar(sqlx::AssertSqlSafe(query_str));
    if let Some(w) = wid { q = q.bind(w); }
    let count: i64 = q.fetch_one(db.pool()).await?;
    Ok(count)
}

/// 获取活跃告警数（通过 devices 表 JOIN 过滤 workspace）
async fn get_active_alarms_count(db: &Database, workspace_id: Option<&str>) -> Result<i64, sqlx::Error> {
    let (query_str, wid) = match workspace_id {
        Some(wid) => (
            "SELECT COUNT(*) FROM device_alarms da JOIN devices d ON da.device_id = d.id WHERE da.is_resolved = 0 AND d.workspace_id = ?",
            Some(wid),
        ),
        None => ("SELECT COUNT(*) FROM device_alarms WHERE is_resolved = 0", None),
    };
    let mut q = sqlx::query_scalar(sqlx::AssertSqlSafe(query_str));
    if let Some(w) = wid { q = q.bind(w); }
    let count: i64 = q.fetch_one(db.pool()).await?;
    Ok(count)
}

/// 确定系统状态
fn determine_system_status(online_devices: i64, total_devices: i64, active_alarms: i64) -> String {
    if active_alarms > 10 {
        "error".to_string()
    } else if active_alarms > 0
        || (total_devices > 0 && (online_devices as f64 / total_devices as f64) < 0.8)
    {
        "warning".to_string()
    } else {
        "healthy".to_string()
    }
}

/// 获取系统运行时间
async fn get_system_uptime() -> Result<i64, Box<dyn std::error::Error>> {
    // 这里应该从系统获取实际的运行时间
    // 目前返回模拟数据：7天
    Ok(7 * 24 * 3600)
}

/// 获取今日消息数
async fn get_today_messages_count(_db: &Database) -> Result<i64, sqlx::Error> {
    // 这里应该从消息日志表获取今日消息数
    // 目前返回模拟数据
    Ok(1250)
}

/// 获取月度增长数据
async fn get_monthly_growth(_db: &Database) -> Result<MonthlyGrowth, sqlx::Error> {
    // 这里应该计算本月相比上月的增长
    // 目前返回模拟数据
    Ok(MonthlyGrowth { devices: 12, messages: 350 })
}

/// 获取 CPU 使用率
async fn get_cpu_usage() -> Result<f64, Box<dyn std::error::Error>> {
    // 这里应该从系统监控服务获取实际的 CPU 使用率
    // 可以使用 sysinfo crate 或其他系统监控库
    Ok(25.5)
}

/// 获取内存使用率
async fn get_memory_usage() -> Result<f64, Box<dyn std::error::Error>> {
    // 这里应该从系统监控服务获取实际的内存使用率
    Ok(68.2)
}

/// 获取磁盘使用率
async fn get_disk_usage() -> Result<f64, Box<dyn std::error::Error>> {
    // 这里应该从系统监控服务获取实际的磁盘使用率
    Ok(45.8)
}

/// 获取网络入站流量
async fn get_network_inbound() -> Result<i64, Box<dyn std::error::Error>> {
    // 这里应该从系统监控服务获取实际的网络流量
    Ok(1024 * 1024 * 50) // 50MB
}

/// 获取网络出站流量
async fn get_network_outbound() -> Result<i64, Box<dyn std::error::Error>> {
    // 这里应该从系统监控服务获取实际的网络流量
    Ok(1024 * 1024 * 30) // 30MB
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/stats", get(get_dashboard_stats))
        .route("/metrics", get(get_dashboard_metrics))
}
