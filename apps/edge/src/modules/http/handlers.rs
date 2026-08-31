use axum::{
    Json,
    extract::{Path, State},
};
use std::sync::Arc;
use tinyiothub_web::response::{ApiResponse, ApiResponseBuilder};

use crate::app_state::AppState;
use crate::shared::error::EdgeError;

type JsonResponse = Json<ApiResponse<serde_json::Value>>;

// ── 1. GET /api/v1/health ───────────────────────────────────────

pub async fn get_health(State(state): State<Arc<AppState>>) -> JsonResponse {
    let report = state.health_service.generate_report().await;
    ApiResponseBuilder::success(serde_json::to_value(report).unwrap_or_default())
}

// ── 2. GET /api/v1/things ──────────────────────────────────────

pub async fn get_things(State(state): State<Arc<AppState>>) -> JsonResponse {
    match state.thing_service.list_things(None).await {
        Ok(things) => ApiResponseBuilder::success(serde_json::to_value(things).unwrap_or_default()),
        Err(e) => ApiResponseBuilder::error(e.to_string()),
    }
}

// ── 3. GET /api/v1/things/{id} ─────────────────────────────────

pub async fn get_thing(State(state): State<Arc<AppState>>, Path(thing_id): Path<String>) -> JsonResponse {
    match state.thing_service.get_thing(&thing_id).await {
        Ok(thing) => ApiResponseBuilder::success(serde_json::to_value(thing).unwrap_or_default()),
        Err(e) => ApiResponseBuilder::error(e.to_string()),
    }
}

// ── 4. GET /api/v1/things/{id}/properties ──────────────────────

pub async fn get_thing_properties(State(state): State<Arc<AppState>>, Path(thing_id): Path<String>) -> JsonResponse {
    match state.thing_service.get_thing(&thing_id).await {
        Ok(_thing) => {
            let properties = serde_json::json!({"thing_id": thing_id, "status": "online"});
            ApiResponseBuilder::success(properties)
        }
        Err(e) => ApiResponseBuilder::error(e.to_string()),
    }
}

// ── 5. POST /api/v1/things/{id}/properties ─────────────────────

pub async fn post_thing_properties(
    State(state): State<Arc<AppState>>,
    Path(thing_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> JsonResponse {
    match state.thing_service.get_thing(&thing_id).await {
        Ok(_) => {
            tracing::info!(thing_id = %thing_id, ?body, "Property write requested");
            ApiResponseBuilder::success(serde_json::json!({"updated": true}))
        }
        Err(e) => ApiResponseBuilder::error(e.to_string()),
    }
}

// ── 6. POST /api/v1/things/{id}/command ────────────────────────

pub async fn post_thing_command(
    State(state): State<Arc<AppState>>,
    Path(thing_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> JsonResponse {
    match state.command_service.execute(&thing_id, &body).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"executed": true})),
        Err(e) => ApiResponseBuilder::error(e.to_string()),
    }
}

// ── 7. GET /api/v1/drivers ──────────────────────────────────────

pub async fn get_drivers(State(state): State<Arc<AppState>>) -> JsonResponse {
    match state.driver_service.list_drivers().await {
        Ok(drivers) => ApiResponseBuilder::success(serde_json::to_value(drivers).unwrap_or_default()),
        Err(e) => ApiResponseBuilder::error(e.to_string()),
    }
}

// ── 8. POST /api/v1/drivers/scan ────────────────────────────────

pub async fn post_driver_scan(State(state): State<Arc<AppState>>) -> JsonResponse {
    match state.driver_service.scan_all().await {
        Ok(things) => ApiResponseBuilder::success(serde_json::json!({
            "scanned": true,
            "things_found": things.len(),
            "things": things,
        })),
        Err(e) => {
            if matches!(e, EdgeError::ScanBusy) {
                ApiResponseBuilder::error_with_code(409, e.to_string())
            } else {
                ApiResponseBuilder::error(e.to_string())
            }
        }
    }
}

// ── 9. GET /api/v1/alarms ───────────────────────────────────────

pub async fn get_alarms(State(_state): State<Arc<AppState>>) -> JsonResponse {
    // In production: query system_alarms table
    ApiResponseBuilder::success(serde_json::json!([]))
}

// ── 10. GET /api/v1/config ──────────────────────────────────────

pub async fn get_config(State(state): State<Arc<AppState>>) -> JsonResponse {
    match state.config_service.get_merged_config().await {
        Ok(config) => ApiResponseBuilder::success(serde_json::to_value(config).unwrap_or_default()),
        Err(e) => ApiResponseBuilder::error(e.to_string()),
    }
}

// ── 11. PUT /api/v1/config ──────────────────────────────────────

pub async fn put_config(State(state): State<Arc<AppState>>, Json(body): Json<serde_json::Value>) -> JsonResponse {
    match state.config_service.apply_cloud_config(&body).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"applied": true})),
        Err(e) => ApiResponseBuilder::error(e.to_string()),
    }
}

// ── 12. GET /api/v1/offline-buffer ──────────────────────────────

pub async fn get_offline_buffer(State(state): State<Arc<AppState>>) -> JsonResponse {
    let status = state.offline_buffer.get_status().await;
    ApiResponseBuilder::success(serde_json::to_value(status).unwrap_or_default())
}
