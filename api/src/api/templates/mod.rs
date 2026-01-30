use crate::shared::app_state::AppState;
use axum::Router;

mod management;

pub fn create_router() -> Router<AppState> {
    Router::new().merge(management::create_router())
}
