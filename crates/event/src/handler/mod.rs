// Events API module
// Provides REST API endpoints for event querying and statistics

use axum::{
    Router,
    routing::{get, post},
};

use crate::EventState;

pub mod overview;
pub mod query;
pub mod real_time;

/// Create the events API router.
///
/// Generic over the composition layer's state `S`: handlers extract
/// `State<EventState>`, which axum derives from `S` via `FromRef`.
///
/// Boundary: the `/security/*` and `/sse*` routes from the old
/// `modules::event::handler::create_router` stay in cloud
/// (`shared::event::http`) with the security plane and SSE manager they
/// depend on.
pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    EventState: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/", get(query::get_events))
        .route("/", post(query::create_event))
        .route("/real-time", get(real_time::get_real_time_events))
        .route("/real-time/status", get(real_time::get_status_summary))
        .route("/real-time/{id}/acknowledge", post(real_time::acknowledge_event))
        .route("/overview", get(overview::get_event_overview))
}
