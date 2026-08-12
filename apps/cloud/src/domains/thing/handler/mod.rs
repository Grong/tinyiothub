// Thing API routes

use axum::{
    Router,
    routing::{delete, get, post, put},
};

use crate::shared::app_state::AppState;

pub mod actions;
pub mod crud;
pub mod import_export;
pub mod resources;

/// Create the thing API router at /api/v1/things.
///
/// Generic over the composition layer's state `S`: handlers extract
/// `State<AppState>`, which axum derives from `S` via `FromRef`.
pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AppState: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/", get(crud::list_things))
        .route("/", post(crud::create_thing))
        .route("/{id}", get(crud::get_thing))
        .route("/{id}", put(crud::update_thing))
        .route("/{id}", delete(crud::delete_thing))
        .route("/{id}/ontology", get(crud::get_thing_ontology))
        .route("/{id}/profile", get(crud::get_thing_profile))
        .route("/{id}/tree", get(crud::get_thing_tree))
        .route("/{id}/actions/{action_name}/invoke", post(actions::invoke_action))
        .route("/{id}/actions/{action_name}/confirm", post(actions::confirm_action))
        .route("/import/dtdl", post(import_export::import_dtdl))
        .route("/import/wot", post(import_export::import_wot))
        .route("/templates/{id}/export/dtdl", get(import_export::export_dtdl))
        .route("/resources/unassigned", get(resources::list_unassigned_resources))
        .route("/{id}/resources", post(resources::attach_resource))
        .route("/{id}/resources/{rid}", delete(resources::detach_resource))
}
