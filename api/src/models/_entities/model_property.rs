use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "model_property")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub model_identifier: String,  // 关联物模型的identifier
    pub identifier: String,       // power_status
    pub name: String,
    pub description: Option<String>,
    pub data_type: String,        // bool, double, enum
    pub access_mode: String,      // rw, r
    pub data_specs: Value,        // 数据规范
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::thing_model::Entity",
        from = "Column::ModelIdentifier",
        to = "super::thing_model::Column::Identifier"
    )]
    ThingModel,
    
    #[sea_orm(has_many = "super::device_property_value::Entity")]
    DeviceValues,
    
    #[sea_orm(has_many = "super::module_item::Entity")]
    ModuleItems,
}

impl Related<super::thing_model::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ThingModel.def()
    }
}

impl Related<super::device_property_value::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::DeviceValues.def()
    }
}

impl Related<super::module_item::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ModuleItems.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}