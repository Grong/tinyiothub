use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use tinyiothub_web::response::{ApiResponse, ApiResponseBuilder};

use crate::{
    modules::gateway::{
        service::PairingError,
        types::{PairingRequest, PairingResponse},
    },
    shared::{app_state::AppState, security::jwt::Claims},
};

fn extract_client_ip(headers: &HeaderMap) -> Option<String> {
    if let Some(forwarded) = headers.get("x-forwarded-for") {
        if let Ok(val) = forwarded.to_str() {
            return val.split(',').next().map(|s| s.trim().to_string());
        }
    }
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(val) = real_ip.to_str() {
            return Some(val.to_string());
        }
    }
    None
}

pub async fn pair_device(
    State(state): State<AppState>,
    claims: Claims,
    headers: HeaderMap,
    Json(req): Json<PairingRequest>,
) -> Result<Json<ApiResponse<PairingResponse>>, (StatusCode, Json<ApiResponse<PairingResponse>>)> {
    let client_ip = extract_client_ip(&headers);

    match state.gateway_service.pair_device(&claims.user_id, client_ip.as_deref(), req).await {
        Ok(response) => Ok(ApiResponseBuilder::success(response)),
        Err(e) => {
            let (status, code, msg) = match &e {
                PairingError::CodeNotFound => (StatusCode::NOT_FOUND, 404, e.to_string()),
                PairingError::CodeExpired => (StatusCode::GONE, 410, e.to_string()),
                PairingError::InvalidCode => (StatusCode::BAD_REQUEST, 400, e.to_string()),
                PairingError::TooManyAttempts | PairingError::TooManyAttemptsIp => {
                    (StatusCode::TOO_MANY_REQUESTS, 429, e.to_string())
                }
                PairingError::ServiceBusy => (StatusCode::SERVICE_UNAVAILABLE, 503, e.to_string()),
                _ => {
                    (StatusCode::INTERNAL_SERVER_ERROR, -1, "配对暂时失败，请稍后重试".to_string())
                }
            };
            Err((status, ApiResponseBuilder::error_with_code(code, msg)))
        }
    }
}
