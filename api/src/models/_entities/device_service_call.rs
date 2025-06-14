use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "device_service_call")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(primary_key)]
    pub device_id: i64,
    #[sea_orm(primary_key)]
    pub identifier: String,
    pub display_name: String,
    pub input_parameters: Value,
    pub status: String, // pending, executing, completed, failed
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub output_result: Option<Value>,
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
        belongs_to = "super::model_service::Entity",
        from = "Column::Identifier",
        to = "super::model_service::Column::Identifier"
    )]
    ModelService,
}

impl Related<super::device::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Device.def()
    }
}

impl Related<super::model_service::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ModelService.def()
    }
}

impl ActiveModelBehavior for ActiveModel {

}