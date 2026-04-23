use axum::{extract::State, routing::get, Json, Router};
use serde::{Deserialize, Serialize};

use crate::{response::ApiResponse, state::WebState};

/// Basic health status response.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct HealthStatus {
    pub status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub uptime_seconds: u64,
}

/// Create the health check router.
pub fn create_router<S: WebState>() -> Router<S> {
    Router::new().route("/", get(get_health::<S>))
}

/// Basic health check endpoint.
async fn get_health<S: WebState>(State(_state): State<S>) -> Json<ApiResponse<HealthStatus>> {
    let health = HealthStatus {
        status: "healthy".to_string(),
        timestamp: chrono::Utc::now(),
        uptime_seconds: 0,
    };

    Json(ApiResponse {
        code: 0,
        msg: String::new(),
        result: Some(health),
    })
}
