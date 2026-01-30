pub mod management;
pub mod query;

use axum::{
    routing::{get, post},
    Router,
};

use crate::shared::app_state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        // 查询接口
        .route("/", get(query::list_alarms))
        .route("/:id", get(query::get_alarm))
        .route("/statistics", get(query::get_alarm_statistics))
        // 管理接口
        .route("/:id/acknowledge", post(management::acknowledge_alarm))
        .route("/:id/resolve", post(management::resolve_alarm))
        .route("/batch-acknowledge", post(management::batch_acknowledge))
        .route("/batch-resolve", post(management::batch_resolve))
}
