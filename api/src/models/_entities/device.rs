use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// 设备配置结构体 (辅助结构，不直接存储)
#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub heartbeat_interval: u32, // 心跳间隔 (秒)
    pub data_report_interval: u32, // 数据上报间隔 (秒)
    pub max_retries: u8,         // 最大重试次数
    pub protocol_version: String, // 协议版本
    pub timezone: String,        // 设备时区
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub ip_address: Option<String>,   // IP地址 (可选)
    pub port: Option<u16>,            // 端口
    pub mac_address: Option<String>,  // MAC地址
    pub connection_type: String,      // MQTT, HTTP, CoAP, TCP, etc.
    pub broker_url: Option<String>,   // MQTT代理地址
    pub topic_prefix: Option<String>, // MQTT主题前缀
    pub keepalive: u16,               // 保持连接时间
    pub qos: u8,                      // MQTT QoS等级
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub username: Option<String>,     // 用户名
    pub password: Option<String>,     // 密码 (加密存储)
    pub access_key: Option<String>,   // 访问密钥
    pub secret_key: Option<String>,   // 密钥 (加密存储)
    pub certificate: Option<String>,  // 证书路径或内容
    pub encryption: String,           // 加密方式: none, tls, dtls, etc.
    pub auth_method: String,          // 认证方式: token, cert, basic
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "device")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub tenant_id: String,
    pub model_identifier: String,
    pub identifier: String,
    pub display_name: String,
    // 设备配置信息
    pub config: Value,          // 通用配置 (JSON)
    pub network_config: Value,  // 网络配置 (JSON)
    pub security_config: Value, // 安全配置 (JSON)
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::thing_model::Entity",
        from = "Column::ModelIdentifier",
        to = "super::thing_model::Column::Identifier"
    )]
    ThingModel,
    #[sea_orm(has_many = "super::device_property_value::Entity")]
    PropertyValues,
    #[sea_orm(has_many = "super::device_event_record::Entity")]
    EventRecords,
    #[sea_orm(has_many = "super::device_service_call::Entity")]
    ServiceCalls,
}

impl Related<super::thing_model::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ThingModel.def()
    }
}

impl Related<super::device_property_value::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PropertyValues.def()
    }
}

impl Related<super::device_event_record::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::EventRecords.def()
    }
}

impl Related<super::device_service_call::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ServiceCalls.def()
    }
}

impl ActiveModelBehavior for ActiveModel {
}