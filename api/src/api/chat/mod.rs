pub mod proxy;
pub mod skills;
pub mod types;
pub mod heartbeat;

use axum::{routing::{get, post}, Router};
use crate::shared::app_state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/stream", post(proxy::chat_stream))
        .route("/history", get(proxy::chat_history))
        .route("/abort", post(proxy::chat_abort))
        .route("/sessions", get(proxy::list_sessions))
        .route("/sessions/:session_key/label", post(proxy::update_session_label))
        .route("/sessions/:session_key", axum::routing::delete(proxy::delete_session))
        .nest("/agents", heartbeat::create_router())
}
