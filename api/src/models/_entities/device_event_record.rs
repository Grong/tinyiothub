use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "device_event_record")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(primary_key)]
    pub device_id: i64,
    #[sea_orm(primary_key)]
    pub identifier: String,
    pub title: String,
    pub content: String,
    pub level: String,
    pub r#type: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::device::Entity",
        from = "Column::DeviceId",
        to = "super::device::Column::Id"
    )]
    Device,
    
    #[sea_orm(
        belongs_to = "super::model_event::Entity",
        from = "Column::Identifier",
        to = "super::model_event::Column::Identifier"
    )]
    ModelEvent,
}

impl Related<super::device::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Device.def()
    }
}

impl Related<super::model_event::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ModelEvent.def()
    }
}

impl ActiveModelBehavior for ActiveModel {

}