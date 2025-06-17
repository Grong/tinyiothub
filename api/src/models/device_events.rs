use loco_rs::model::{ModelError, ModelResult};
use sea_orm::{entity::prelude::*, ActiveValue::Set};
use serde::{Deserialize, Serialize};
use ulid::Ulid;
pub use super::_entities::device_events::{ActiveModel, Model, Entity};
pub type DeviceEvents = Entity;

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            id: Set(Ulid::new().to_string()),
            timestamp: Set(chrono::Utc::now().fixed_offset()),
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
impl Model {

    pub async fn list_paginated(
        db: &DatabaseConnection,
        params: super::ListParams,
    ) -> ModelResult<super::PaginatedResult<Self>> {
        let query = Entity::find();

        let paginator = query.paginate(db, params.limit);
        let total = paginator.num_items().await?;
        let data = paginator.fetch_page(params.page - 1).await?;

        Ok(super::PaginatedResult {
            data: data,
            total,
            page: params.page,
            limit: params.limit,
            pages: (total + params.limit - 1) / params.limit,
            has_more: params.page * params.limit < total,
        })
    }
}

// implement your write-oriented logic here
impl ActiveModel {
    pub async fn record_event(
        db: &DatabaseConnection,
        device_id: &str,
        event_type: &str,
        payload: Json,
    ) -> ModelResult<Model> {
        // 创建事件记录
        let event = ActiveModel {
            device_id: Set(device_id.to_string()),
            event_type: Set(event_type.to_string()),
            payload: Set(payload),
            ..Default::default()
        }
        .insert(db)
        .await?;
        Ok(event)
    }
}

// implement your custom finders, selectors oriented logic here
impl Entity {}



#[derive(Debug, Clone, PartialEq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(10))")]
pub enum EventType {
    #[sea_orm(string_value = "info")]
    Info = 0,
    #[sea_orm(string_value = "warning")]
    Warning = 1,
    #[sea_orm(string_value = "alert")]
    Alert = 2,
}

impl EventType {
    pub fn from_str(s: &str) -> Result<Self, ModelError> {
        match s {
            "info" => Ok(EventType::Info),
            "warning" => Ok(EventType::Warning),
            "alert" => Ok(EventType::Alert),
            _ => Err(ModelError::Message(format!("Invalid event type: {}", s))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(10))")]
pub enum EventSeverity {
    #[sea_orm(string_value = "low")]
    Low = 0,
    #[sea_orm(string_value = "medium")]
    Medium = 1,
    #[sea_orm(string_value = "high")]
    High = 2,
}