use axum::{debug_handler, extract::Query};
use loco_rs::prelude::*;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
pub struct Params {
    pub current_version: Option<String>,
}

#[debug_handler]
async fn version(State(_ctx): State<AppContext>, Query(_query): Query<Params>) -> Result<Response> {
    format::json(json!({
        "version": "1.0.0",
        "release_date": "",
        "release_notes": "",
        "can_auto_update": false,
    }))
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/console/api")
        .add("/version", get(version))
}
