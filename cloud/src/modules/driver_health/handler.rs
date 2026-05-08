use axum::{Json, Router, extract::State, routing::get};
use tinyiothub_web::response::ApiResponseBuilder;

use super::service::DriverHealthService;
use crate::{
    api::middleware::WorkspaceScope,
    shared::{api_response::ApiResponse, app_state::AppState},
};

pub fn create_router() -> Router<AppState> {
    Router::new().route("/drivers", get(list_driver_health))
}

async fn list_driver_health(
    State(_state): State<AppState>,
    WorkspaceScope(workspace_id): WorkspaceScope,
) -> Json<ApiResponse<serde_json::Value>> {
    let ws_id = workspace_id.as_deref().unwrap_or("");
    let registry = tinyiothub_runtime::driver_registry().read();
    let health = DriverHealthService::get_workspace_health(&registry, ws_id);
    ApiResponseBuilder::success(serde_json::to_value(health).unwrap_or_default())
}
