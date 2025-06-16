use sea_orm::{entity::prelude::*, ActiveValue::Set};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ulid::Ulid;

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

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "device")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String, // 设备唯一ID

    pub name: String,
    pub description: Option<String>,
    pub kind: Option<String>, // 设备类型 (如 "thermostat", "camera")

    // 设备配置
    pub config: Value,          // 通用配置
    pub network_config: Value,  // 网络配置
    pub security_config: Value, // 安全配置

    // 时间戳
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub last_seen: Option<DateTimeWithTimeZone>,

    // 状态
    pub is_active: bool,
    pub status: DeviceStatus, // 设备整体状态: normal, warning, error, offline

    pub extensions: Value, // 扩展字段
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::device_event::Entity")]
    Events,
    #[sea_orm(has_many = "super::device_service_call::Entity")]
    ServiceCalls,
    #[sea_orm(has_many = "super::device_property::Entity")]
    Properties,
}

impl Related<super::device_event::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Events.def()
    }
}

impl Related<super::device_service_call::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ServiceCalls.def()
    }
}

impl Related<super::device_property::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Properties.def()
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    fn new() -> Self {
        let now = chrono::Utc::now().fixed_offset();
        Self {
            id: Set(Ulid::new().to_string()), // 自动生成ID
            created_at: Set(now),
            updated_at: Set(now),
            is_active: Set(true),
            status: Set(DeviceStatus::Normal),
            config: Set(serde_json::json!({})),
            network_config: Set(serde_json::json!({})),
            security_config: Set(serde_json::json!({})),
            extensions: Set(serde_json::json!({})),
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
