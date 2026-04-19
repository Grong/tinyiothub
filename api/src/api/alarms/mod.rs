pub mod query;
pub mod recent;

use axum::{
    routing::{get},
    Router,
};

use crate::shared::app_state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        // 查询接口
        .route("/", get(query::list_alarms))
        .route("/statistics", get(query::get_alarm_statistics))
        .merge(recent::create_router())
        .route("/{id}", get(query::get_alarm))
        // 告警确认和解决应该通过 PATCH /alarms/{id} 更新 acknowledged/resolved 字段
}
