use axum::{Json, Router, routing::get};
use tinyiothub_web::{
    middleware::workspace::WorkspaceScope,
    response::{ApiResponse, ApiResponseBuilder},
};

use super::service::DriverHealthService;

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/drivers", get(list_driver_health))
}

async fn list_driver_health(WorkspaceScope(workspace_id): WorkspaceScope) -> Json<ApiResponse<serde_json::Value>> {
    let ws_id = workspace_id.as_deref().unwrap_or("");
    let registry = tinyiothub_runtime::driver_registry().read();
    let health = DriverHealthService::get_workspace_health(&registry, ws_id);
    ApiResponseBuilder::success(serde_json::to_value(health).unwrap_or_default())
}
