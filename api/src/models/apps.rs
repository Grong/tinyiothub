use crate::models::_entities::apps::{self, Column};
use loco_rs::model::{ModelError, ModelResult};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

pub use super::_entities::apps::{ActiveModel, Entity, Model};
pub type Apps = Entity;

#[derive(Debug, Deserialize)]
pub struct ListParams {
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_limit")]
    pub limit: u64,
    pub name: Option<String>,
    pub is_created_by_me: Option<bool>,
}

fn default_page() -> u64 { 1 }
fn default_limit() -> u64 { 30 }

#[derive(Debug, Serialize)]
pub struct PaginatedResult<T> {
    pub data: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub pages: u64,
    pub has_more: bool,
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if !insert && self.updated_at.is_unchanged() {
            let mut this = self;
            this.updated_at = sea_orm::ActiveValue::Set(chrono::Utc::now().into());
            Ok(this)
        } else {
            Ok(self)
        }
    }
}

// implement your read-oriented logic here
impl Model {
    pub async fn find_by_name(db: &DatabaseConnection, name: &str) -> ModelResult<Self> {
        let app = apps::Entity::find()
            .filter(apps::Column::Name.eq(name))
            .one(db)
            .await?;
        app.ok_or_else(|| ModelError::EntityNotFound)
    }

    pub async fn list_paginated(
        db: &DatabaseConnection, 
        params: ListParams
    ) -> ModelResult<PaginatedResult<Self>> {
        let mut query = Entity::find();

        // 添加名称过滤
        if let Some(name) = params.name {
            query = query.filter(Column::Name.contains(name));
        }

        if let Some(is_created_by_me) = params.is_created_by_me {
            query = query.filter(Column::CreatedBy.eq(is_created_by_me));
        }

        let paginator = query.paginate(db, params.limit);
        let total = paginator.num_items().await?;
        let data = paginator.fetch_page(params.page - 1).await?;


        Ok(PaginatedResult {
            data: data,
            total,
            page: params.page,
            limit: params.limit,
            pages: (total + params.limit - 1) / params.limit,
            has_more:  params.page * params.limit < total,
        })
    }
}

// implement your write-oriented logic here
impl ActiveModel {}

// implement your custom finders, selectors oriented logic here
impl Entity {}
