use std::fmt::Display;

use loco_rs::model::{ModelError, ModelResult};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "device_property")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub device_id: i64,
    pub identifier: String,
    pub display_name: String,
    pub value: String,
    pub data_type: PropertyDataType,
    pub status: PropertyStatus,
    #[sea_orm(column_type = "Text")]
    pub data_specs: Option<String>,
    pub updated_at: DateTimeWithTimeZone,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PropertyThresholds {
    pub high: f64,
    pub low: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PropertyValidRange {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PropertyDataSpecs {
    pub thresholds: Option<PropertyThresholds>,
    pub valid_range: Option<PropertyValidRange>,
    pub expected_value: Option<bool>, // 期望值 数据类型为布尔值时使用
    pub options: Option<String>,      // 枚举值 数据类型为枚举值时使用
    pub unit: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(10))")]
pub enum PropertyDataType {
    #[sea_orm(string_value = "boolean")]
    Boolean,
    #[sea_orm(string_value = "int")]
    Int,
    #[sea_orm(string_value = "float")]
    Float,
    #[sea_orm(string_value = "double")]
    Double,
    #[sea_orm(string_value = "number")]
    Number,
    #[sea_orm(string_value = "enum")]
    Enum,
    #[sea_orm(string_value = "image")]
    Image,
    #[sea_orm(string_value = "video")]
    Video,
    #[sea_orm(string_value = "string")]
    String,
}

impl PropertyDataType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "boolean" => PropertyDataType::Boolean,
            "int" => PropertyDataType::Int,
            "float" => PropertyDataType::Float,
            "double" => PropertyDataType::Double,
            "number" => PropertyDataType::Number,
            "enum" => PropertyDataType::Enum,
            "image" => PropertyDataType::Image,
            "video" => PropertyDataType::Video,
            _ => PropertyDataType::String,
        }
    }
}

#[derive(Debug, Clone, PartialEq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(10))")]
pub enum PropertyStatus {
    #[sea_orm(string_value = "normal")]
    Normal = 0,
    #[sea_orm(string_value = "alarm")]
    Alarm = 1,
    #[sea_orm(string_value = "unknown")]
    Unknown = 2,
}

impl Display for PropertyStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PropertyStatus::Normal => write!(f, "正常"),
            PropertyStatus::Alarm => write!(f, "告警"),
            PropertyStatus::Unknown => write!(f, "未知"),
        }
    }
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

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    pub fn get_data_specs(&self) -> Option<PropertyDataSpecs> {
        self.data_specs
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
    }

    pub fn set_data_specs(&mut self, specs: Option<PropertyDataSpecs>) {
        self.data_specs = specs.map(|s| serde_json::to_string(&s).unwrap_or_default());
    }

    pub fn get_thresholds(&self) -> Option<PropertyThresholds> {
        self.data_specs
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .and_then(|specs: PropertyDataSpecs| specs.thresholds)
    }

    pub fn set_thresholds(&mut self, thresholds: Option<PropertyThresholds>) {
        let mut specs = self.get_data_specs().unwrap_or_default();
        specs.thresholds = thresholds;
        self.data_specs = Some(serde_json::to_string(&specs).unwrap_or_default());
    }

    pub fn get_valid_range(&self) -> Option<PropertyValidRange> {
        self.data_specs
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .and_then(|specs: PropertyDataSpecs| specs.valid_range)
    }

    pub fn set_valid_range(&mut self, valid_range: Option<PropertyValidRange>) {
        let mut specs = self.get_data_specs().unwrap_or_default();
        specs.valid_range = valid_range;
        self.data_specs = Some(serde_json::to_string(&specs).unwrap_or_default());
    }

    pub fn get_expected_value(&self) -> Option<bool> {
        self.data_specs
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .and_then(|specs: PropertyDataSpecs| specs.expected_value)
    }

    pub fn set_expected_value(&mut self, expected_value: Option<bool>) {
        let mut specs = self.get_data_specs().unwrap_or_default();
        specs.expected_value = expected_value;
        self.data_specs = Some(serde_json::to_string(&specs).unwrap_or_default());
    }

    pub fn get_options(&self) -> Option<Vec<String>> {
        self.data_specs
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .and_then(|specs: PropertyDataSpecs| {
                serde_json::from_str(&specs.options.unwrap_or_default()).ok()
            })
    }

    pub fn set_options(&mut self, options: Option<String>) {
        let mut specs = self.get_data_specs().unwrap_or_default();
        specs.options = options;
        self.data_specs = Some(serde_json::to_string(&specs).unwrap_or_default());
    }

    pub async fn find_prop_by_device_id(
        db: &DatabaseConnection,
        device_id: &str,
        identifier: &str,
    ) -> ModelResult<Self> {
        let app = Entity::find()
            .filter(
                Column::DeviceId
                    .eq(device_id)
                    .and(Column::Identifier.eq(identifier)),
            )
            .one(db)
            .await?;
        app.ok_or_else(|| ModelError::EntityNotFound)
    }

    pub async fn find_props_by_device_id(
        db: &DatabaseConnection,
        device_id: &str,
    ) -> ModelResult<Vec<Self>> {
        let app = Entity::find()
            .filter(Column::DeviceId.eq(device_id))
            .all(db)
            .await?;
        Ok(app)
    }
}
