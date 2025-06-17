#![allow(clippy::missing_errors_doc)]
#![allow(clippy::unnecessary_struct_initialization)]
#![allow(clippy::unused_async)]
use crate::models::{
    device_templates::Model, ListParams,
};
use axum::{debug_handler, extract::Query};
use loco_rs::prelude::*;
#[debug_handler]
pub async fn list(
    State(ctx): State<AppContext>,
    Query(params): Query<ListParams>,
) -> Result<Response> {
    let result = Model::list_paginated(&ctx.db, params).await?;
    format::json(result)
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("iot/api/device-templates")
        .add("/", get(list))
}
