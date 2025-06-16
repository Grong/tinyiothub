use axum::debug_handler;
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct InitResponse {
    status: String,
}

#[debug_handler]
async fn init(State(_ctx): State<AppContext>) -> Result<Response> {
    // 返回默认完成
    let init_status = InitResponse {
        status: "finished".to_string(),
    };
    format::json(init_status)
}

pub fn routes() -> Routes {
    Routes::new().prefix("/console/api").add("/init", get(init))
}
