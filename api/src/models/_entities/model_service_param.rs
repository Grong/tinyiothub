use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "service_param")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub service_id: i32,
    pub identifier: String,
    pub name: String,
    pub data_type: String,
    pub required: bool,
    pub data_specs: Value,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::model_service::Entity",
        from = "Column::ServiceId",
        to = "super::model_service::Column::Id"
    )]
    ModelService,
}

impl Related<super::model_service::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ModelService.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}