use loco_rs::model::ModelResult;
use sea_orm::{entity::prelude::*, ActiveValue::Set};
use serde::{Deserialize, Serialize};

pub use super::_entities::devices::{ActiveModel, Model, Entity};
pub type Devices = Entity;

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        let now = chrono::Utc::now().fixed_offset();
        Self {
            created_at: Set(now),
            updated_at: Set(now),
            is_active: Set(true),
            config: Set(Some(serde_json::json!({}))),
            network_config: Set(Some(serde_json::json!({}))),
            security_config: Set(Some(serde_json::json!({}))),
            extensions: Set(Some(serde_json::json!({}))),
            ..ActiveModelTrait::default()
        }
    }

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

// implement your read-oriented logic here
impl Model {}

// implement your write-oriented logic here
impl ActiveModel {
    pub async fn create_device(
        db: &DatabaseConnection,
        name: &str,
        description: Option<&str>,
    ) -> ModelResult<Model> {
        let device = Self {
            name: Set(name.to_string()),
            description: Set(description.map(|s| s.to_string())),
            ..Default::default()
        }
        .insert(db)
        .await?;
        Ok(device)
    }
}

// implement your custom finders, selectors oriented logic here
impl Entity {}

#[derive(Debug, Clone, PartialEq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(10))")]
pub enum DeviceStatus {
    #[sea_orm(string_value = "normal")]
    Normal = 0,
    #[sea_orm(string_value = "warning")]
    Warning = 1,
    #[sea_orm(string_value = "error")]
    Error = 2,
    #[sea_orm(string_value = "offline")]
    Offline = 3,
}