
use axum::debug_handler;
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct SetupResponse {
    step: String,
}

#[debug_handler]
async fn setup(State(_ctx): State<AppContext>) -> Result<Response> {
    // 返回默认完成
    let setup = SetupResponse {
        step: "finished".to_string(),
    };
    format::json(setup)
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/console/api")
        .add("/setup", get(setup))
}