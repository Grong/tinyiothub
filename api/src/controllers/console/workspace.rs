use axum::debug_handler;
use loco_rs::prelude::*;
use serde_json::json;

#[debug_handler]
async fn current(State(_ctx): State<AppContext>) -> Result<Response> {
    format::json(json!({
        "id": "1",
        "name": "Workspace 1",
        "plan": "",
        "created_at": "2021-01-01",
        "updated_at": "2021-01-01",
        "role": "admin",
        "status": "active",
    }))
}

#[debug_handler]
async fn list(State(_ctx): State<AppContext>) -> Result<Response> {
    format::json(json!({
        "workspaces": [
            {
                "id": "1",
                "name": "Workspace 1",
                "plan": "",
                "created_at": "2021-01-01",
                "updated_at": "2021-01-01",
                "role": "admin",
                "status": "active",
                "current": true,
            }
        ],
    }))
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/console/api")
        .add("/workspaces/current", get(current))
        .add("/workspaces", get(list))
}
