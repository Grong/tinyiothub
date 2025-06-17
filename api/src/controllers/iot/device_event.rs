#![allow(clippy::missing_errors_doc)]
#![allow(clippy::unnecessary_struct_initialization)]
#![allow(clippy::unused_async)]
use axum::debug_handler;
use loco_rs::prelude::*;
use serde_json::Value;
use crate::models::prelude::*;

#[debug_handler]
pub async fn record_event(
    Path((device_id, event_type)): Path<(String, String)>,
    State(ctx): State<AppContext>,
    Json(payload): Json<Value>,
) -> Result<Response> {
    let event = DeviceEventActiveModel::record_event(&ctx.db, &device_id, &event_type, payload).await?;

    format::json(event)
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("iot/api/devices")
        .add("/{device_id}/events/{event_type}", post(record_event))
}
