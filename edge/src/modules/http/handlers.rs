use axum::{Json, extract::{Path, State}};
use std::sync::Arc;
use tinyiothub_web::response::{ApiResponse, ApiResponseBuilder};

use crate::app_state::AppState;
use crate::shared::error::EdgeError;

type JsonResponse = Json<ApiResponse<serde_json::Value>>;

// ── 1. GET /api/v1/health ───────────────────────────────────────

pub async fn get_health(
    State(state): State<Arc<AppState>>,
) -> JsonResponse {
    let report = state.health_service.generate_report().await;
    ApiResponseBuilder::success(serde_json::to_value(report).unwrap_or_default())
}

// ── 2. GET /api/v1/devices ──────────────────────────────────────

pub async fn get_devices(
    State(state): State<Arc<AppState>>,
) -> JsonResponse {
    match state.device_service.list_devices(None).await {
        Ok(devices) => {
            ApiResponseBuilder::success(serde_json::to_value(devices).unwrap_or_default())
        }
        Err(e) => ApiResponseBuilder::error(e.to_string()),
    }
}

// ── 3. GET /api/v1/devices/{id} ─────────────────────────────────

pub async fn get_device(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> JsonResponse {
    match state.device_service.get_device(&device_id).await {
        Ok(device) => {
            ApiResponseBuilder::success(serde_json::to_value(device).unwrap_or_default())
        }
        Err(e) => ApiResponseBuilder::error(e.to_string()),
    }
}

// ── 4. GET /api/v1/devices/{id}/properties ──────────────────────

pub async fn get_device_properties(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> JsonResponse {
    match state.device_service.get_device(&device_id).await {
        Ok(_device) => {
            let properties = serde_json::json!({"device_id": device_id, "status": "online"});
            ApiResponseBuilder::success(properties)
        }
        Err(e) => ApiResponseBuilder::error(e.to_string()),
    }
}

// ── 5. POST /api/v1/devices/{id}/properties ─────────────────────

pub async fn post_device_properties(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> JsonResponse {
    match state.device_service.get_device(&device_id).await {
        Ok(_) => {
            tracing::info!(device_id = %device_id, ?body, "Property write requested");
            ApiResponseBuilder::success(serde_json::json!({"updated": true}))
        }
        Err(e) => ApiResponseBuilder::error(e.to_string()),
    }
}

// ── 6. POST /api/v1/devices/{id}/command ────────────────────────

pub async fn post_device_command(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> JsonResponse {
    match state.command_service.execute(&device_id, &body).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"executed": true})),
        Err(e) => ApiResponseBuilder::error(e.to_string()),
    }
}

// ── 7. GET /api/v1/drivers ──────────────────────────────────────

pub async fn get_drivers(
    State(state): State<Arc<AppState>>,
) -> JsonResponse {
    match state.driver_service.list_drivers().await {
        Ok(drivers) => {
            ApiResponseBuilder::success(serde_json::to_value(drivers).unwrap_or_default())
        }
        Err(e) => ApiResponseBuilder::error(e.to_string()),
    }
}

// ── 8. POST /api/v1/drivers/scan ────────────────────────────────

pub async fn post_driver_scan(
    State(state): State<Arc<AppState>>,
) -> JsonResponse {
    match state.driver_service.scan_all().await {
        Ok(devices) => ApiResponseBuilder::success(serde_json::json!({
            "scanned": true,
            "devices_found": devices.len(),
            "devices": devices,
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

pub async fn get_alarms(
    State(_state): State<Arc<AppState>>,
) -> JsonResponse {
    // In production: query system_alarms table
    ApiResponseBuilder::success(serde_json::json!([]))
}

// ── 10. GET /api/v1/config ──────────────────────────────────────

pub async fn get_config(
    State(state): State<Arc<AppState>>,
) -> JsonResponse {
    match state.config_service.get_merged_config().await {
        Ok(config) => {
            ApiResponseBuilder::success(serde_json::to_value(config).unwrap_or_default())
        }
        Err(e) => ApiResponseBuilder::error(e.to_string()),
    }
}

// ── 11. PUT /api/v1/config ──────────────────────────────────────

pub async fn put_config(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> JsonResponse {
    match state.config_service.apply_cloud_config(&body).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"applied": true})),
        Err(e) => ApiResponseBuilder::error(e.to_string()),
    }
}

// ── 12. GET /api/v1/offline-buffer ──────────────────────────────

pub async fn get_offline_buffer(
    State(state): State<Arc<AppState>>,
) -> JsonResponse {
    let status = state.offline_buffer.get_status().await;
    ApiResponseBuilder::success(serde_json::to_value(status).unwrap_or_default())
}
