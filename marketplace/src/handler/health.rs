use axum::{extract::State, Json, Router};
use tinyiothub_web::response::ApiResponseBuilder;

use crate::AppState;
use crate::types::HealthResponse;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", axum::routing::get(health_check))
}

async fn health_check(State(state): State<AppState>) -> Json<tinyiothub_web::response::ApiResponse<HealthResponse>> {
    let last_sync = state.cache.get_last_sync().ok().flatten();

    let is_degraded = state.cache.is_cold() || last_sync.is_none();

    let status = if is_degraded {
        "degraded"
    } else if let Some(ts) = last_sync {
        let now = chrono::Utc::now().timestamp();
        if now - ts > 3600 { "degraded" } else { "ok" }
    } else {
        "ok"
    };

    let response = HealthResponse {
        status: status.to_string(),
        last_sync: last_sync.and_then(|ts| {
            chrono::DateTime::from_timestamp(ts, 0).map(|dt| dt.to_rfc3339())
        }),
        reason: if status == "degraded" { Some("rate_limit_exhausted".to_string()) } else { None },
    };

    ApiResponseBuilder::success(response)
}
