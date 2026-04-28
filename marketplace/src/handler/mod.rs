use axum::Router;
use crate::AppState;

mod health;
mod drivers;
mod templates;

pub fn routes() -> Router<AppState> {
    Router::new()
        .merge(health::routes())
        .nest("/api/v1", v1_routes())
}

fn v1_routes() -> Router<AppState> {
    Router::new()
        .merge(drivers::routes())
        .merge(templates::routes())
}
