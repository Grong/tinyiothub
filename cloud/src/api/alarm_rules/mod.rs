pub mod crud;

use axum::{
    routing::{delete, get, post, put},
    Router,
};

use crate::shared::app_state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(crud::list_alarm_rules))
        .route("/", post(crud::create_alarm_rule))
        .route("/{id}", get(crud::get_alarm_rule))
        .route("/{id}", put(crud::update_alarm_rule))
        .route("/{id}", delete(crud::delete_alarm_rule))
        .route("/{id}/toggle", post(crud::toggle_alarm_rule))
}
