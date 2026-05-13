use axum::{extract::State, Json};
use std::sync::Arc;
use tinyiothub_web::response::{ApiResponseBuilder, ApiResponse};

use crate::modules::gateway::service::{GatewayService, PairingError};
use crate::modules::gateway::types::{PairingRequest, PairingResponse};

pub async fn pair_device(
    State(service): State<Arc<GatewayService>>,
    Json(req): Json<PairingRequest>,
) -> Json<ApiResponse<PairingResponse>> {
    let user_id = "anonymous";

    match service.pair_device(user_id, req).await {
        Ok(response) => ApiResponseBuilder::success(response),
        Err(e) => {
            let (code, msg) = match &e {
                PairingError::CodeNotFound => (404, e.to_string()),
                PairingError::InvalidCode => (400, e.to_string()),
                PairingError::TooManyAttempts => (429, e.to_string()),
                PairingError::ServiceBusy => (503, e.to_string()),
                _ => (-1, "配对暂时失败，请稍后重试".to_string()),
            };
            ApiResponseBuilder::error_with_code(code, msg)
        }
    }
}
