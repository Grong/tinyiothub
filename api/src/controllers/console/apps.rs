#![allow(clippy::missing_errors_doc)]
#![allow(clippy::unnecessary_struct_initialization)]
#![allow(clippy::unused_async)]
use crate::{models::{
    apps::{ActiveModel, Entity, Model},
    users, ListParams,
}, views::app::{AppDetailWithSiteResponse, SiteResponse}};
use axum::{debug_handler, extract::Query};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Params {
    pub name: Option<String>,
    pub description: Option<String>,
    pub llms: Option<String>,
    pub mode: Option<String>,
    pub icon_type: Option<String>,
    pub icon: Option<String>,
}

impl Params {
    fn update(&self, item: &mut ActiveModel) {
        item.name = Set(self.name.clone());
        item.description = Set(self.description.clone());
        item.llms = Set(self.llms.clone());
        item.mode = Set(self.mode.clone());
        item.icon_type = Set(self.icon_type.clone());
        item.icon = Set(self.icon.clone());
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InputTracingRequest {
    pub enabled: bool,
    pub tracing_provider: String,
    pub tracing_config: Option<String>,
}

impl InputTracingRequest {
    fn update(&self, item: &mut ActiveModel) {
        item.tracing = Set(Some(json!({
            "enabled": self.enabled,
            "tracing_provider": self.tracing_provider,
            "tracing_config": self.tracing_config,
        }).to_string()));
    }
}

async fn load_item(ctx: &AppContext, id: i32) -> Result<Model> {
    let item = Entity::find_by_id(id).one(&ctx.db).await?;
    item.ok_or_else(|| Error::NotFound)
}

#[debug_handler]
pub async fn list(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Query(params): Query<ListParams>,
) -> Result<Response> {
    let user = users::Model::find_by_pid(&ctx.db, &auth.claims.pid).await?;
    let result = Model::list_paginated(&ctx.db, params, user.id).await?;
    format::json(result)
}

#[debug_handler]
pub async fn add(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Json(params): Json<Params>,
) -> Result<Response> {
    let user = users::Model::find_by_pid(&ctx.db, &auth.claims.pid).await?;
    let mut item = ActiveModel {
        created_by: Set(user.id),
        ..Default::default()
    };
    println!("item: {:?}", item);
    params.update(&mut item);
    let item = item.insert(&ctx.db).await?;
    format::json(item)
}

#[debug_handler]
pub async fn update(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
    Json(params): Json<Params>,
) -> Result<Response> {
    let item = load_item(&ctx, id).await?;
    let mut item = item.into_active_model();
    params.update(&mut item);
    let item = item.update(&ctx.db).await?;
    format::json(item)
}

#[debug_handler]
pub async fn remove(Path(id): Path<i32>, State(ctx): State<AppContext>) -> Result<Response> {
    load_item(&ctx, id).await?.delete(&ctx.db).await?;
    format::empty()
}

#[debug_handler]
pub async fn get_one(Path(id): Path<i32>, State(ctx): State<AppContext>) -> Result<Response> {
    let item = load_item(&ctx, id).await?;
    let site = SiteResponse {
        access_token: "".to_string(),
        title: "".to_string(),
        description: "".to_string(),
        icon_type: "".to_string(),
        icon: "".to_string(),
        icon_background: "".to_string(),
        icon_url: "".to_string(),
        app_base_url: item.api_base_url.clone().unwrap_or_default(),
    };
    format::json(AppDetailWithSiteResponse {
        id: item.id,
        name: item.name.unwrap_or_default(),
        description: item.description.unwrap_or_default(),
        mode: item.mode.unwrap_or_default(),
        enable_site: item.enable_site,
        enable_api: item.enable_api,
        api_rpm: item.api_rpm,
        api_rph: item.api_rph,
        tracing: item.tracing.unwrap_or_default(),
        api_base_url: item.api_base_url.clone().unwrap_or_default(),
        site: site,
        created_at: item.created_at,
        updated_at: item.updated_at,
        created_by: item.created_by,
        updated_by: item.updated_by,
        access_mode: item.access_mode.unwrap_or_default(),
        icon_type: item.icon_type.unwrap_or_default(),
        icon: item.icon.unwrap_or_default(),
        icon_background: item.icon_background.unwrap_or_default(),
    })
}

#[debug_handler]
pub async fn get_api_keys(Path(id): Path<i32>, State(ctx): State<AppContext>) -> Result<Response> {
    // let item = load_item(&ctx, id).await?;
    // let api_keys = item.api_keys.clone();
    format::json(json!({
        "data": [
            {
                "id": 1,
                "token": "lo-95ec80d7-cb60-4b70-9b4b-9ef74cb88758",
                "created_at": "2021-01-01T00:00:00Z",
                "last_used_at": "2021-01-01T00:00:00Z"
            }
        ]
    }))
}

#[debug_handler]
pub async fn get_tracing(Path(id): Path<i32>, State(ctx): State<AppContext>) -> Result<Response> {
    let item = load_item(&ctx, id).await?;
    let tracing = match item.tracing {
        Some(tracing) => tracing,
        None => return format::json(json!({
            "enabled": false,
            "tracing_provider": null,
        })),
    };
    format::json(json!(tracing))
}

#[debug_handler]
pub async fn update_tracing(Path(id): Path<i32>, State(ctx): State<AppContext>, Json(params): Json<InputTracingRequest>) -> Result<Response> {
    let item = load_item(&ctx, id).await?;
    let mut item = item.into_active_model();
    params.update(&mut item);
    item.update(&ctx.db).await?;
    format::json(json!({"result": "success"}))
}   

pub fn routes() -> Routes {
    Routes::new()
        .prefix("console/api/apps/")
        .add("/", get(list))
        .add("/", post(add))
        .add("{id}", get(get_one))
        .add("{id}", delete(remove))
        .add("{id}", put(update))
        .add("{id}", patch(update))
        .add("{id}/api-keys", get(get_api_keys))
        .add("{id}/trace", get(get_tracing))
        .add("{id}/trace", post(update_tracing))
}
