use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "thing_model")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub identifier: String,  // dtmi:com:example:Thermostat;1
    
    pub name: String,
    pub description: Option<String>,
    pub version: String,
    pub schema_version: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub extensions: Value,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::model_property::Entity")]
    Properties,
    
    #[sea_orm(has_many = "super::model_service::Entity")]
    Services,
    
    #[sea_orm(has_many = "super::model_event::Entity")]
    Events,
    
    #[sea_orm(has_many = "super::model_module::Entity")]
    Modules,
    
    #[sea_orm(has_many = "super::device::Entity")]
    Devices,
}


impl Related<super::model_property::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Properties.def()
    }
}

impl Related<super::model_service::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Services.def()
    }
}

impl Related<super::model_event::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Events.def()
    }
}

impl Related<super::model_module::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Modules.def()
    }
}

impl Related<super::device::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Devices.def()
    }
}

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