pub mod handlers;

pub use handlers::*;

use axum::{routing::{get, post}, Router};

use crate::api::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_tags).post(create_tag))
        .route("/{id}", get(get_tag).put(update_tag).delete(delete_tag))
        .route("/search", get(search_tags))
        .route("/stats", get(get_tag_stats))
        .route("/bindings", post(create_tag_binding).delete(delete_tag_binding))
        .route("/bindings/batch", post(batch_create_bindings).delete(batch_delete_bindings))
        .route("/bindings/target/{target_id}", get(get_target_bindings))
        .route("/bindings/tag/{tag_id}", get(get_tag_bindings))
}
