use crate::models::device_property::PropertyDataType;
use crate::models::{device, device_template};
use crate::prelude::*;
use sea_orm::ActiveValue::Set;
use sea_orm::IntoActiveModel;
use serde_json::{json, Value};

pub struct DeviceService;

impl DeviceService {
    /// 直接创建设备
    pub async fn create_device(
        db: &DatabaseConnection,
        name: &str,
        description: Option<&str>,
    ) -> Result<device::Model> {
        let device = device::ActiveModel {
            name: Set(name.to_string()),
            description: Set(description.map(|s| s.to_string())),
            ..Default::default()
        }
        .insert(db)
        .await?;

        Ok(device)
    }

    /// 从模板创建设备
    pub async fn create_device_from_template(
        db: &DatabaseConnection,
        template_id: i32,
        name: &str,
    ) -> Result<device::Model> {
        let template = device_template::Entity::find_by_id(template_id)
            .one(db)
            .await?
            .ok_or_else(|| Error::NotFound)?;

        // 解析模板
        let template_json: Value = template.template;

        // 创建基础设备
        let mut device = device::ActiveModel {
            name: Set(name.to_string()),
            kind: Set(Some(template.name.clone())),
            ..Default::default()
        };

        // 应用模板属性
        if let Some(properties) = template_json["properties"].as_object() {
            for (key, prop) in properties {
                let prop_active = DevicePropertyActiveModel {
                    identifier: Set(key.to_string()),
                    display_name: Set(prop["name"].as_str().unwrap_or("").to_string()),
                    data_type: Set(PropertyDataType::from_str(&prop["dataType"].to_string())),
                    data_specs: Set(Some(prop["dataSpecs"].to_string())),
                    ..Default::default()
                };
                prop_active.insert(db).await?;
            }
        }

        // 应用配置
        if let Some(config) = template_json["extensions"].as_object() {
            device.config = Set(config.clone().into());
        }

        let device = device.insert(db).await?;
        Ok(device)
    }

    /// 获取设备状态
    pub async fn get_device_status(db: &DatabaseConnection, device_id: &str) -> Result<Value> {
        let device = device::Entity::find_by_id(device_id)
            .one(db)
            .await?
            .ok_or_else(|| Error::NotFound)?;

        let mut status = json!({
            "id": device.id,
            "name": device.name,
            "is_active": device.is_active,
            "last_seen": device.last_seen,
            "properties": {}
        });

        let properties = DevicePropertyModel::find_props_by_device_id(db, device_id).await?;
        for prop in properties {
            status["properties"][prop.identifier] = json!({
                "name": prop.display_name,
                "status": prop.status,
                "value": prop.value,
                "type": prop.data_type,
                "specs": prop.data_specs
            });
        }
        Ok(status)
    }
}
