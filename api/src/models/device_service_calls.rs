use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
pub use super::_entities::device_service_calls::{ActiveModel, Model, Entity};
pub type DeviceServiceCalls = Entity;

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
impl ActiveModel {}

// implement your custom finders, selectors oriented logic here
impl Entity {}


#[derive(Debug, Clone, PartialEq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(15))")]
pub enum ServiceStatus {
    #[sea_orm(string_value = "pending")]
    Pending = 0,
    #[sea_orm(string_value = "executing")]
    Executing = 1,
    #[sea_orm(string_value = "completed")]
    Completed = 2,
    #[sea_orm(string_value = "failed")]
    Failed = 3,
}