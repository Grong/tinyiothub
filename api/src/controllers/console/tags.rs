#![allow(clippy::missing_errors_doc)]
#![allow(clippy::unnecessary_struct_initialization)]
#![allow(clippy::unused_async)]
use axum::{debug_handler, extract::Query};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

use crate::models::{_entities::tags::{ActiveModel, Entity, Model}, users, prelude::*};

#[derive(Clone, Debug, Deserialize)]
pub struct QueryParams {
    pub r#type: Option<String>,
    pub keyword: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddParams {
    pub name: Option<String>,
    pub r#type: Option<String>,
    pub tenant_id: Option<String>,
}

impl AddParams {
    fn update(&self, item: &mut ActiveModel) {
        item.name = Set(self.name.clone());
        item.r#type = Set(self.r#type.clone());
        item.tenant_id = Set(self.tenant_id.clone());
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TagBindingParams {
    pub tag_ids: Vec<i32>,
    pub target_id: i32,
    pub r#type: String,
}

async fn load_item(ctx: &AppContext, id: i32) -> Result<Model> {
    let item = Entity::find_by_id(id).one(&ctx.db).await?;
    item.ok_or_else(|| Error::NotFound)
}

#[debug_handler]
pub async fn list(
    State(ctx): State<AppContext>,
    Query(query): Query<QueryParams>,
) -> Result<Response> {
    let tags = Model::get_tags(&ctx.db, "".to_string(), query.r#type, query.keyword).await?;
    format::json(tags)
}

#[debug_handler]
pub async fn add(
    auth: auth::JWT,
    State(ctx): State<AppContext>,
    Json(params): Json<AddParams>,
) -> Result<Response> {
    let user = users::Model::find_by_pid(&ctx.db, &auth.claims.pid).await?;
    let mut item = ActiveModel {
        created_by: Set(user.id),
        ..Default::default()
    };
    params.update(&mut item);
    let item = item.insert(&ctx.db).await?;
    format::json(item)
}

#[debug_handler]
pub async fn update(
    Path(id): Path<i32>,
    State(ctx): State<AppContext>,
    Json(params): Json<AddParams>,
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
    format::json(load_item(&ctx, id).await?)
}

#[debug_handler]
pub async fn create_tag_binding(
    State(ctx): State<AppContext>,
    Json(params): Json<TagBindingParams>,
) -> Result<Response> {
    let tag_binding = TagBindingActiveModel::create_tag_binding(&ctx.db, params.tag_ids, params.target_id).await?;
    format::json(tag_binding)
}

#[debug_handler]
pub async fn remove_tag_binding(
    State(ctx): State<AppContext>,
    Json(params): Json<TagBindingParams>,
) -> Result<Response> {
    let tag_binding = TagBindingActiveModel::remove_tag_binding(&ctx.db, params.tag_ids, params.target_id).await?;
    format::json(tag_binding)
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("console/api")
        .add("/tags", get(list))
        .add("/tags", post(add))
        .add("/tag-bindings/create", post(create_tag_binding))
        .add("/tag-bindings/remove", post(remove_tag_binding))
}
