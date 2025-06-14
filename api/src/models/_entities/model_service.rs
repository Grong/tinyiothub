use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "model_service")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(primary_key)]
    pub model_identifier: String,
    #[sea_orm(primary_key)]
    pub identifier: String,
    pub name: String,
    pub description: Option<String>,
    pub call_type: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::thing_model::Entity",
        from = "Column::ModelIdentifier",
        to = "super::thing_model::Column::Identifier"
    )]
    ThingModel,
    
    #[sea_orm(has_many = "super::model_service_param::Entity")]
    Params,
    
    #[sea_orm(has_many = "super::device_service_call::Entity")]
    ServiceCalls,
    
    #[sea_orm(has_many = "super::module_item::Entity")]
    ModuleItems,
}

impl Related<super::thing_model::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ThingModel.def()
    }
}

impl Related<super::model_service_param::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Params.def()
    }
}

impl Related<super::device_service_call::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ServiceCalls.def()
    }
}

impl Related<super::module_item::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ModuleItems.def()
    }
}

impl ActiveModelBehavior for ActiveModel {

}