/// Gateway API Handlers
/// 网关管理 API 实现

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;

use crate::dto::entity::gateway::{Gateway, GatewayDevice, CreateGatewayRequest, UpdateGatewayRequest};
use crate::dto::response::api_response::ApiResponse;
use crate::shared::app_state::AppState;

#[derive(Debug, Deserialize)]
pub struct GatewayQuery {
    pub status: Option<String>,
}

/// List all gateways
pub async fn list_gateways(
    State(state): State<AppState>,
    Query(query): Query<GatewayQuery>,
) -> Result<Json<ApiResponse<Vec<Gateway>>>, StatusCode> {
    match Gateway::find_all(state.database(), query.status.as_deref()).await {
        Ok(gateways) => Ok(Json(ApiResponse {
            code: 0,
            msg: "Success".to_string(),
            result: Some(gateways),
        })),
        Err(e) => {
            tracing::error!("Failed to list gateways: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get a single gateway
pub async fn get_gateway(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Gateway>>, StatusCode> {
    match Gateway::find_by_id(state.database(), &id).await {
        Ok(Some(gateway)) => Ok(Json(ApiResponse {
            code: 0,
            msg: "Success".to_string(),
            result: Some(gateway),
        })),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get gateway: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Create a new gateway
pub async fn create_gateway(
    State(state): State<AppState>,
    Json(payload): Json<CreateGatewayRequest>,
) -> Result<Json<ApiResponse<Gateway>>, StatusCode> {
    match Gateway::create(state.database(), &payload).await {
        Ok(gateway) => Ok(Json(ApiResponse {
            code: 0,
            msg: "Gateway created".to_string(),
            result: Some(gateway),
        })),
        Err(e) => {
            tracing::error!("Failed to create gateway: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Update a gateway
pub async fn update_gateway(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<UpdateGatewayRequest>,
) -> Result<Json<ApiResponse<Gateway>>, StatusCode> {
    match Gateway::update(state.database(), &id, &payload).await {
        Ok(gateway) => Ok(Json(ApiResponse {
            code: 0,
            msg: "Gateway updated".to_string(),
            result: Some(gateway),
        })),
        Err(e) => {
            tracing::error!("Failed to update gateway: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Delete a gateway
pub async fn delete_gateway(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    match Gateway::delete(state.database(), &id).await {
        Ok(_) => Ok(Json(ApiResponse {
            code: 0,
            msg: "Gateway deleted".to_string(),
            result: Some(()),
        })),
        Err(e) => {
            tracing::error!("Failed to delete gateway: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get devices under a gateway
pub async fn get_gateway_devices(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<String>>>, StatusCode> {
    match GatewayDevice::get_gateway_devices(state.database(), &id).await {
        Ok(devices) => Ok(Json(ApiResponse {
            code: 0,
            msg: "Success".to_string(),
            result: Some(devices),
        })),
        Err(e) => {
            tracing::error!("Failed to get gateway devices: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Update gateway status (online/offline)
pub async fn update_gateway_status(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<()>>, StatusCode> {
    let status = payload.get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("offline");
    
    match Gateway::update_status(state.database(), &id, status).await {
        Ok(_) => Ok(Json(ApiResponse {
            code: 0,
            msg: "Status updated".to_string(),
            result: Some(()),
        })),
        Err(e) => {
            tracing::error!("Failed to update gateway status: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
