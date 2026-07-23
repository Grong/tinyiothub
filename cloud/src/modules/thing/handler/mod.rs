// Thing API routes

use axum::{
    Router,
    routing::{delete, get, post, put},
};

use crate::shared::app_state::AppState;

pub mod crud;

/// Create the thing API router at /api/v1/things
pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(crud::list_things))
        .route("/", post(crud::create_thing))
        .route("/{id}", get(crud::get_thing))
        .route("/{id}", put(crud::update_thing))
        .route("/{id}", delete(crud::delete_thing))
        .route("/{id}/ontology", get(crud::get_thing_ontology))
        .route("/{id}/profile", get(crud::get_thing_profile))
        .route("/{id}/tree", get(crud::get_thing_tree))
}
