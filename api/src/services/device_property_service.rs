use crate::models::devices::{DeviceStatus};
use crate::models::device_properties::{PropertyDataType, PropertyStatus};
use crate::prelude::*;
use serde_json::{json, Value};

pub struct DevicePropertyService;

impl DevicePropertyService {
    /// 更新属性值并计算状态
    pub async fn update_property(
        db: &DatabaseConnection,
        device_id: &str,
        property_name: &str,
        value: Value,
    ) -> Result<DeviceModel> {
        let device = Device::find_by_id(device_id)
            .one(db)
            .await?
            .ok_or_else(|| Error::NotFound)?;

        let prop =
            DevicePropertyModel::find_prop_by_device_id(db, device_id, property_name).await?;
        let mut prop_active = prop.into_active_model();
        prop_active.value = Set(value.to_string());
        prop_active.update(db).await?;

        let overall_status = DeviceStatus::Normal;

        let mut active_device = device.into_active_model();
        active_device.updated_at = Set(chrono::Utc::now().fixed_offset());
        active_device.status = Set(overall_status as i32);

        let updated = active_device.update(db).await?;
        Ok(updated)
    }

    /// 根据属性类型和值计算状态
    fn calculate_property_status(property: &DevicePropertyModel, value: &Value) -> PropertyStatus {
        match property.get_data_type() {
            PropertyDataType::Number
            | PropertyDataType::Float
            | PropertyDataType::Double
            | PropertyDataType::Int => {
                if let Some(num) = value.as_f64() {
                    // 检查阈值
                    if let Some(thresholds) = property.get_thresholds() {
                        let high = thresholds.high;
                        let low = thresholds.low;

                        if num > high {
                            return PropertyStatus::Alarm;
                        } else if num < low {
                            return PropertyStatus::Alarm;
                        }
                    }

                    // 检查是否在正常范围内
                    if let Some(range) = property.get_valid_range() {
                        let min = range.min;
                        let max = range.max;

                        if num < min || num > max {
                            return PropertyStatus::Alarm;
                        }
                    }
                }
                PropertyStatus::Normal
            }
            PropertyDataType::Boolean => {
                if let Some(b) = value.as_bool() {
                    if let Some(expected) = property.get_expected_value() {
                        if expected != b {
                            return PropertyStatus::Alarm;
                        }
                    }
                }
                PropertyStatus::Normal
            }
            PropertyDataType::Enum => {
                if let Some(s) = value.as_str() {
                    if let Some(options) = property.get_options() {
                        if !options.iter().any(|opt| opt == s) {
                            return PropertyStatus::Alarm;
                        }
                    }
                }
                PropertyStatus::Normal
            }
            PropertyDataType::Image | PropertyDataType::Video => PropertyStatus::Normal,
            _ => PropertyStatus::Normal,
        }
    }

    /// 设置属性阈值
    pub async fn set_property_thresholds(
        db: &DatabaseConnection,
        device_id: &str,
        property_name: &str,
        thresholds: Value,
    ) -> Result<DevicePropertyModel> {
        let prop =
            DevicePropertyModel::find_prop_by_device_id(db, device_id, property_name).await?;
        let status = Self::calculate_property_status(&prop, &thresholds);
        let mut prop_active = prop.into_active_model();
        prop_active.status = Set(status as i32);
        let updated = prop_active.update(db).await?;
        Ok(updated)
    }

    
    /// 获取设备属性历史
    pub async fn get_property_history(
        db: &DatabaseConnection,
        device_id: &str,
        property_name: &str,
        hours: Option<i32>,
    ) -> Result<Value> {
        let device = Device::find_by_id(device_id)
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
}
