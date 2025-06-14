use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "model_module")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub model_identifier: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::thing_model::Entity",
        from = "Column::ModelIdentifier",
        to = "super::thing_model::Column::Identifier"
    )]
    ThingModel,
    
    #[sea_orm(has_many = "super::module_item::Entity")]
    Items,
}

impl Related<super::thing_model::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ThingModel.def()
    }
}

impl Related<super::module_item::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Items.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}