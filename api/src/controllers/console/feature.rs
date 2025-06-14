use crate::extensions::config_ext::ConfigExt;
use axum::debug_handler;
use loco_rs::prelude::*;

#[debug_handler]
async fn features(State(ctx): State<AppContext>) -> Result<Response> {
    let features = ctx.config.get_features();
    format::json(features)
}

#[debug_handler]
async fn system_features(State(ctx): State<AppContext>) -> Result<Response> {
    let features = ctx.config.get_system_features();
    format::json(features)
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/console/api")
        .add("/features", get(features))
        .add("/system-features", get(system_features))
}
