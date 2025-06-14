use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "event_param")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub event_id: i32,
    pub identifier: String,
    pub name: String,
    pub data_type: String,
    pub data_specs: Value,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::model_event::Entity",
        from = "Column::EventId",
        to = "super::model_event::Column::Id"
    )]
    ModelEvent,
}

impl Related<super::model_event::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ModelEvent.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}