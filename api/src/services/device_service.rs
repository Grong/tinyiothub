use crate::models::prelude::*;
use crate::prelude::*;
use sea_orm::ActiveValue::Set;
use serde_json::{json, Value};

pub struct DeviceService;

impl DeviceService {
    /// 直接创建设备
    pub async fn create_device(
        db: &DatabaseConnection,
        name: &str,
        description: Option<&str>,
    ) -> Result<DeviceModel> {
        let device = DeviceActiveModel {
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
        template_id: &str,
        name: &str,
    ) -> Result<DeviceModel> {
        let template = DeviceTemplate::find_by_id(template_id)
            .one(db)
            .await?
            .ok_or_else(|| Error::NotFound)?;

        // 解析模板
        let template_json: Value = template.template;

        // 创建基础设备
        let mut device = DeviceActiveModel {
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
                    data_type: Set(prop["dataType"].as_i64().unwrap_or(0) as i32),
                    data_specs: Set(Some(prop["dataSpecs"].to_string())),
                    ..Default::default()
                };
                prop_active.insert(db).await?;
            }
        }

        // 应用配置
        if let Some(config) = template_json["extensions"].as_object() {
            device.config = Set(Some(config.clone().into()));
        }

        let device = device.insert(db).await?;
        Ok(device)
    }

    /// 获取设备状态
    pub async fn get_device_status(db: &DatabaseConnection, device_id: &str) -> Result<Value> {
        let device = Device::find_by_id(device_id)
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

     /// 获取设备完整状态
     pub async fn get_full_status(db: &DatabaseConnection, device_id: &str) -> Result<Value> {
        let device = Device::find_by_id(device_id)
            .one(db)
            .await?
            .ok_or_else(|| Error::NotFound)?;

        let mut status = json!({
            "id": device.id,
            "name": device.name,
            "kind": device.kind,
            "is_active": device.is_active,
            "status": device.status,
            "last_seen": device.last_seen,
            "properties": {}
        });

        let properties = DevicePropertyModel::find_props_by_device_id(db, device_id).await?;
        for prop in properties {
            status["properties"][prop.identifier] = json!({
                "value": prop.value,
                "status": prop.status,
                "type": prop.data_type,
                "specs": prop.data_specs
            });
        }
        // 添加配置信息
        status["config"] = device.config.clone().unwrap_or_default();
        status["network_config"] = device.network_config.clone().unwrap_or_default();

        Ok(status)
    }

    /// 检查设备健康状况
    pub async fn check_health(db: &DatabaseConnection, device_id: &str) -> Result<Value> {
        let device = Device::find_by_id(device_id)
            .one(db)
            .await?
            .ok_or_else(|| Error::NotFound)?;

        let mut health = json!({
            "device_id": device.id,
            "status": device.status,
            "last_seen": device.last_seen,
            "is_active": device.is_active,
            "issues": []
        });

        // 检查连接状态
        if !device.is_active {
            health["issues"].as_array_mut().unwrap().push(json!({
                "type": "connectivity",
                "severity": "critical",
                "message": "设备离线"
            }));
        } else if let Some(last_seen) = device.last_seen {
            let offline_duration = chrono::Utc::now().fixed_offset() - last_seen;
            if offline_duration.num_minutes() > 30 {
                health["issues"].as_array_mut().unwrap().push(json!({
                    "type": "connectivity",
                    "severity": "warning",
                    "message": format!("设备超过30分钟未更新，最后在线: {}", last_seen)
                }));
            }
        }

        // 检查属性状态
        let properties = DevicePropertyModel::find_props_by_device_id(db, device_id).await?;
        for prop in properties {
            if prop.status == PropertyStatus::Alarm as i32 {
                health["issues"].as_array_mut().unwrap().push(json!({
                    "type": "property",
                    "property": prop.identifier,
                    "severity": "critical",
                    "status": prop.status,
                    "message": format!("属性 '{}' 状态异常: {}", prop.identifier, prop.status)
                }));
            }
        }

        // 更新整体健康状态
        let has_critical = health["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["severity"] == "critical");

        let has_warning = health["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["severity"] == "warning");

        health["status"] = if has_critical {
            json!("critical")
        } else if has_warning {
            json!("warning")
        } else {
            json!("healthy")
        };

        Ok(health)
    }
}
