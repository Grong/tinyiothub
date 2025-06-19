use std::result;

use crate::{models::_entities::apps::{self, Column}, views::tag::TagResponse};
use loco_rs::model::{ModelError, ModelResult};
use sea_orm::entity::prelude::*;
use crate::views::app::AppResponse;

pub use super::_entities::apps::{ActiveModel, Entity, Model};
pub type Apps = Entity;

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

    pub async fn get_tags(db: &DatabaseConnection, app_id: i32) -> ModelResult<Vec<super::_entities::tags::Model>> {
        let tag_bindings = super::tag_bindings::TagBindings::find()
            .filter(super::tag_bindings::Column::TargetId.eq(app_id))
            .find_with_related(super::_entities::tags::Entity)
            .all(db)
            .await?;
        let tags = tag_bindings.iter().map(|(_, tags)| tags.clone()).flatten().collect();
        Ok(tags)
    }

    pub async fn list_paginated(
        db: &DatabaseConnection,
        params: super::ListParams,
        login_user_id: i32,
    ) -> ModelResult<super::PaginatedResult<AppResponse>> {
        let mut query = Entity::find();

        // 添加名称过滤
        if let Some(name) = params.name {
            query = query.filter(Column::Name.contains(name));
        }

        if let Some(is_created_by_me) = params.is_created_by_me {
            if is_created_by_me {
                query = query.filter(Column::CreatedBy.eq(login_user_id));
            }
        }

        let paginator = query.paginate(db, params.limit);
        let total = paginator.num_items().await?;
        let data = paginator.fetch_page(params.page - 1).await?;
        let mut result = vec![];
        for app in data {
            let tags = match Self::get_tags(db, app.id).await {
                Ok(tags) => tags,
                Err(_) => vec![],
            };
            result.push(AppResponse {
                id: app.id,
                name: app.name.unwrap_or_default(),
                description: app.description.unwrap_or_default(),
                tags: tags.iter().map(|tag| TagResponse {
                    id: tag.id,
                    name: tag.name.clone().unwrap_or_default(),
                    r#type: tag.r#type.clone().unwrap_or_default(),
                }).collect(),
            });
        }

        Ok(super::PaginatedResult {
            data: result,
            total,
            page: params.page,
            limit: params.limit,
            pages: (total + params.limit - 1) / params.limit,
            has_more: params.page * params.limit < total,
        })
    }
}

// implement your write-oriented logic here
impl ActiveModel {}

// implement your custom finders, selectors oriented logic here
impl Entity {}
