use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "module_item")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub module_id: i32,
    #[sea_orm(primary_key)]
    pub item_type: String,
    #[sea_orm(primary_key)]
    pub item_identifier: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::model_module::Entity",
        from = "Column::ModuleId",
        to = "super::model_module::Column::Id"
    )]
    ModelModule,
    
    #[sea_orm(
        belongs_to = "super::model_service::Entity",
        from = "Column::ItemIdentifier",
        to = "super::model_service::Column::Identifier"
    )]
    ModelService,

    #[sea_orm(
        belongs_to = "super::model_event::Entity",
        from = "Column::ItemIdentifier",
        to = "super::model_event::Column::Identifier"
    )]
    ModelEvent,

    #[sea_orm(
        belongs_to = "super::model_property::Entity",
        from = "Column::ItemIdentifier",
        to = "super::model_property::Column::Identifier"
    )]
    ModelProperty,
}

impl Related<super::model_module::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ModelModule.def()
    }
}

impl Related<super::model_service::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ModelService.def()
    }
}

impl Related<super::model_event::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ModelEvent.def()
    }
}

impl Related<super::model_property::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ModelProperty.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}