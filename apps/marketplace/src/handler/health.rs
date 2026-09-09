use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tinyiothub_web::response::ApiResponseBuilder;

use crate::AppState;
use crate::types::HealthResponse;

pub fn routes() -> Router<AppState> {
    Router::new().route("/health", axum::routing::get(health_check))
}

async fn health_check(State(state): State<AppState>) -> Response {
    let last_sync = state.cache.get_last_sync().ok().flatten();

    // 健康检查要走过真实数据路径：key 存在但内容损坏/格式不兼容时，
    // 数据端点全部 502 而健康检查不能继续报 ok
    let templates = state.cache.get_templates();
    let drivers = state.cache.get_drivers();
    let unreadable = templates.is_err() || drivers.is_err();
    let missing_templates = matches!(templates, Ok(None));
    let missing_drivers = matches!(drivers, Ok(None));

    // 数据是静态本地文件，不会"过期"——不设存活阈值。
    // degraded 表示数据未加载/不可读/部分缺失，reason 按实际原因填写，供 LB/k8s probe 与排障使用。
    let reason = if unreadable {
        Some("cache_unreadable")
    } else if missing_templates && missing_drivers {
        Some("cache_cold")
    } else if missing_templates {
        Some("templates_cache_missing")
    } else if missing_drivers {
        Some("drivers_cache_missing")
    } else if last_sync.is_none() {
        Some("never_synced")
    } else {
        None
    };
    let degraded = reason.is_some();

    let response = HealthResponse {
        status: if degraded {
            "degraded".to_string()
        } else {
            "ok".to_string()
        },
        last_sync: last_sync.and_then(|ts| chrono::DateTime::from_timestamp(ts, 0).map(|dt| dt.to_rfc3339())),
        reason: reason.map(String::from),
    };

    let status_code = if degraded {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };
    (status_code, ApiResponseBuilder::success(response)).into_response()
}
