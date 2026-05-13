use axum::{extract::State, Json};

use crate::modules::gateway::service::PairingError;
use crate::modules::gateway::types::{PairingRequest, PairingResponse};
use crate::shared::app_state::AppState;
use tinyiothub_web::response::{ApiResponse, ApiResponseBuilder};

pub async fn pair_device(
    State(state): State<AppState>,
    Json(req): Json<PairingRequest>,
) -> Json<ApiResponse<PairingResponse>> {
    let user_id = "anonymous";

    match state.gateway_service.pair_device(user_id, req).await {
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
