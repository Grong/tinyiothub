pub mod proxy;
pub mod skills;
pub mod types;

use axum::{routing::{get, post}, Router};
use crate::shared::app_state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/stream", post(proxy::chat_stream))
        .route("/history", get(proxy::chat_history))
        .route("/abort", post(proxy::chat_abort))
}
