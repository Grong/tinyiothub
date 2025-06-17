use std::fmt::Display;

pub use super::_entities::device_properties::{ActiveModel, Column, Entity, Model};
use loco_rs::model::{ModelError, ModelResult};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
pub type DeviceProperties = Entity;

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

// implement your read-oriented logic here
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

    pub fn get_data_type(&self) -> PropertyDataType {
        PropertyDataType::from_i32(self.data_type)
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

// implement your write-oriented logic here
impl ActiveModel {}

// implement your custom finders, selectors oriented logic here
impl Entity {}

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

    pub fn from_i32(i: i32) -> Self {
        match i {
            0 => PropertyDataType::Boolean,
            1 => PropertyDataType::Int,
            2 => PropertyDataType::Float,
            3 => PropertyDataType::Double,
            4 => PropertyDataType::Number,
            5 => PropertyDataType::Enum,
            6 => PropertyDataType::Image,
            7 => PropertyDataType::Video,
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
