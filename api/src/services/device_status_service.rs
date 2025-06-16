use crate::models::{device, device_property::PropertyStatus};
use crate::prelude::*;
use serde_json::{json, Value};

pub struct DeviceStatusService;

impl DeviceStatusService {
    /// 获取设备完整状态
    pub async fn get_full_status(db: &DatabaseConnection, device_id: &str) -> Result<Value> {
        let device = device::Entity::find_by_id(device_id)
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
        status["config"] = device.config.clone();
        status["network_config"] = device.network_config.clone();

        Ok(status)
    }

    /// 获取设备属性历史
    pub async fn get_property_history(
        db: &DatabaseConnection,
        device_id: &str,
        property_name: &str,
        hours: Option<i32>,
    ) -> Result<Value> {
        let device = device::Entity::find_by_id(device_id)
            .one(db)
            .await?
            .ok_or_else(|| Error::NotFound)?;

        let history = json!([]);

        // 过滤指定时间范围内的历史
        let filtered_history = if let Some(h) = hours {
            let cutoff = chrono::Utc::now() - chrono::Duration::hours(h as i64);
            if let Some(history_arr) = history.as_array() {
                let filtered: Vec<_> = history_arr
                    .iter()
                    .filter(|entry| {
                        if let Some(ts) = entry.get("timestamp").and_then(|t| t.as_str()) {
                            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
                                return dt > cutoff;
                            }
                        }
                        false
                    })
                    .cloned()
                    .collect();
                json!(filtered)
            } else {
                history
            }
        } else {
            history
        };

        Ok(json!({
            "device_id": device_id,
            "property": property_name,
            "history": filtered_history
        }))
    }

    /// 检查设备健康状况
    pub async fn check_health(db: &DatabaseConnection, device_id: &str) -> Result<Value> {
        let device = device::Entity::find_by_id(device_id)
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
            if prop.status == PropertyStatus::Alarm {
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
