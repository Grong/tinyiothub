pub mod proxy;
pub mod types;

use axum::{
    Router,
    routing::{get, post},
};

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    crate::domains::agent::AgentState: axum::extract::FromRef<S>,
    std::sync::Arc<tinyiothub_authn::jwt::JwtService>: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/stream", post(proxy::chat_stream))
        .route("/history", get(proxy::chat_history))
        .route("/abort", post(proxy::chat_abort))
        .route("/sessions", get(proxy::list_sessions))
        .route("/sessions/{session_key}/label", post(proxy::update_session_label))
        .route("/sessions/{session_key}", axum::routing::delete(proxy::delete_session))
}
