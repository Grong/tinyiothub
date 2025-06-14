use crate::models::_entities::tags::{ActiveModel, Column, Entity, Model};
use loco_rs::model::ModelResult;
use sea_orm::entity::prelude::*;

pub type Tags = Entity;

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
    pub async fn get_tags(
        db: &DatabaseConnection,
        current_tenant: String,
        r#type: Option<String>,
        keyword: Option<String>,
    ) -> ModelResult<Vec<Model>> {
        let mut query = Entity::find().filter(Column::TenantId.eq(current_tenant.clone()));

        if let Some(r#type) = r#type {
            query = query.filter(Column::Type.eq(r#type));
        }
        if let Some(keyword) = keyword {
            query = query.filter(Column::Name.like(format!("%{}%", keyword)));
        }
        let tags = query.all(db).await?;
        Ok(tags)
    }
}

// implement your write-oriented logic here
impl ActiveModel {}

// implement your custom finders, selectors oriented logic here
impl Entity {}
