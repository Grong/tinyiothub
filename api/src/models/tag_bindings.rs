use loco_rs::model::ModelResult;
use sea_orm::{entity::prelude::*, ActiveValue::Set};
pub use super::_entities::tag_bindings::{ActiveModel, Model, Entity, Column};
pub type TagBindings = Entity;

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
impl Model {}

// implement your write-oriented logic here
impl ActiveModel {

    pub async fn create_tag_binding(
        db: &DatabaseConnection,
        tag_ids: Vec<i32>,
        target_id: i32,
    ) -> ModelResult<()> {

        for tag_id in tag_ids {
            if Entity::check_target_exists(db, target_id, tag_id).await? {
                continue;
            }
            let _tag_binding = Self {
                tag_id: Set(tag_id),
                target_id: Set(target_id),
                ..Default::default()
            }
            .insert(db)
            .await?;
        }

        Ok(())
    }

    pub async fn remove_tag_binding(
        db: &DatabaseConnection,
        tag_ids: Vec<i32>,
        target_id: i32,
    ) -> ModelResult<()> {
        for tag_id in tag_ids {
            let tag_binding = Entity::find()
                .filter(Column::TargetId.eq(target_id))
                .filter(Column::TagId.eq(tag_id))
                .one(db)
                .await?;
            if tag_binding.is_some() {
                tag_binding.unwrap().delete(db).await?;
            }
        }
        Ok(())
    }
}

// implement your custom finders, selectors oriented logic here
impl Entity {

    pub async fn check_target_exists(db: &DatabaseConnection, target_id: i32,tag_id:i32) -> ModelResult<bool> {
        let tag_binding = Self::find()
            .filter(Column::TargetId.eq(target_id))
            .filter(Column::TagId.eq(tag_id))
            .one(db)
            .await?;
        Ok(tag_binding.is_some())
    }
}
