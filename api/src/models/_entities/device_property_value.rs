use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DataSpecs {
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub step: Option<f64>,
    pub unit: String,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "device_property_value")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub device_id: i64,
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(primary_key)]
    pub identifier: String,
    pub display_name: String,
    pub data_type: String,
    pub access_mode: String,
    #[sea_orm(column_type = "Json")]
    pub data_specs: Value,
    pub value: Value,
    pub updated_at: DateTimeWithTimeZone,
    pub created_at: DateTimeWithTimeZone,
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
        belongs_to = "super::model_property::Entity",
        from = "Column::Identifier",
        to = "super::model_property::Column::Identifier"
    )]
    ModelProperty,
}

impl Related<super::device::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Device.def()
    }
}

impl Related<super::model_property::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ModelProperty.def()
    }
}

impl ActiveModelBehavior for ActiveModel {

}