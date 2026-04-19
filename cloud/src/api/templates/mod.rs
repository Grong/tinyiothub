use axum::Router;

use crate::shared::app_state::AppState;

mod management;

pub fn create_router() -> Router<AppState> {
    Router::new().merge(management::create_router())
}
