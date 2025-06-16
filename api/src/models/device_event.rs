use loco_rs::model::{ModelError, ModelResult};
use sea_orm::{entity::prelude::*, ActiveValue::Set};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "device_event")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub device_id: String,
    pub event_type: EventType,
    pub severity: EventSeverity,
    pub payload: Value,
    pub timestamp: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::device::Entity",
        from = "Column::DeviceId",
        to = "super::device::Column::Id"
    )]
    Device,
}

impl Related<super::device::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Device.def()
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        Self {
            timestamp: Set(chrono::Utc::now().fixed_offset()),
            ..ActiveModelTrait::default()
        }
    }
}

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
